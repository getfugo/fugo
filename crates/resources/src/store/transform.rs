//! Transforming resources and realizing them; images; the content of a resource and its publishing.

use super::*;

impl ResourceStore {
    /// Applies `t` to resource `id` (memoized per `(id, t)`). The result is computed lazily
    /// (see [`crate::pipes`]): its links are final except for a `fingerprint` of a pending
    /// resource; [`realize`](Self::realize) or [`content`](Self::content) computes it.
    /// A `fingerprint` of a computed resource is computed at once.
    ///
    /// # Errors
    /// An unreadable source (a `fingerprint` computed at once).
    pub fn transform(&self, id: ResourceId, t: Transform) -> Result<ResourceId, ResourceError> {
        self.transforms
            .get_or_try((id, t.clone()), || pipes::start(self, id, t))
    }

    /// Whether computing `id` waits for phase E5: its chain processes an image (images are
    /// processed in E6, outside the renders). Any other pending `fingerprint` is
    /// computed when the template asks for it, as Go computes it when its links are read.
    #[must_use]
    pub fn waits_for_e5(&self, id: ResourceId) -> bool {
        let mut id = id;
        loop {
            let r = self.resource(id);
            match &r.origin {
                Origin::Transformed { from, .. } => id = *from,
                Origin::Meta { from } => id = *from,
                Origin::Image { .. } => return true,
                Origin::Asset { .. }
                | Origin::Bundle { .. }
                | Origin::Remote { .. }
                | Origin::Named => {
                    return matches!(r.body, Body::PendingImage(_));
                }
            }
        }
    }

    /// Resource `id` with a pending transform computed (see [`crate::pipes`]); `id` keeps its
    /// id, its record is replaced by the computed one.
    ///
    /// # Errors
    /// A failing transform ([`ResourceError::Pipe`]) or an unreadable source.
    pub fn realize(&self, id: ResourceId) -> Result<Arc<Resource>, ResourceError> {
        pipes::realize(self, id)
    }

    /// Replaces the record of `id` (a computed pending transform).
    pub(crate) fn replace(&self, id: ResourceId, r: Resource) {
        self.arena.write().unwrap_or_else(PoisonError::into_inner)[id] = Arc::new(r);
    }

    /// Resource `id` under another name, title and params (front matter metadata, see
    /// [`crate::meta`]); `id` itself when nothing changes.
    pub(crate) fn with_meta(
        &self,
        id: ResourceId,
        name: String,
        title: String,
        params: Params,
    ) -> ResourceId {
        let src = self.resource(id);
        if name == src.name && title == src.title && params == src.params {
            return id;
        }
        let params_key = serde_json::to_string(params.as_map()).unwrap_or_default();
        let key = (id, name.clone(), title.clone(), params_key);
        let result: Result<ResourceId, std::convert::Infallible> =
            self.metas.get_or_try(key, || {
                Ok(self.push(NewResource {
                    origin: Origin::Meta { from: id },
                    media_type: src.media_type.clone(),
                    name,
                    name_normalized: Some(src.name_normalized.clone()),
                    title,
                    params,
                    data: src.data.clone(),
                    lang: src.lang,
                    target: src.target.clone(),
                    link: src.link.clone(),
                    body: src.body.clone(),
                    policy: src.policy,
                    kind: Some(src.kind),
                }))
            });
        match result {
            Ok(id) => id,
            Err(never) => match never {},
        }
    }

    /// What the [`ImageQueue`] reads for image resource `id`: its file, its operation, or its
    /// bytes (a remote image, a QR code, …), which the queue then holds under the name of the
    /// resource's file. `None` for other resources, a transform not computed yet, or a store
    /// without a queue.
    #[must_use]
    pub fn image_input(&self, id: ResourceId) -> Option<ImageInput> {
        let r = self.resource(id);
        if r.kind != ResourceKind::Image {
            return None;
        }
        match &r.body {
            Body::File(p) => Some(ImageInput::File(p.clone())),
            Body::PendingImage(op) => Some(ImageInput::Op(*op)),
            Body::Bytes(b) => self
                .cfg
                .images
                .as_ref()
                .map(|q| q.add_memory(paths::base(r.target.as_str()), Arc::clone(b))),
            Body::Pending => None,
        }
    }

    /// `.Width` and `.Height` of an image resource: a processed image's planned size, else the
    /// size in the source's header (read once per file, no pixels decoded). `None` for other
    /// resources and for images whose header cannot be read.
    #[must_use]
    pub fn image_size(&self, r: &Resource) -> Option<(u32, u32)> {
        if r.kind != ResourceKind::Image {
            return None;
        }
        match &r.body {
            Body::PendingImage(op) => self
                .cfg
                .images
                .as_ref()
                .and_then(|q| q.get(*op))
                .map(|e| (e.width, e.height)),
            Body::File(p) => {
                if let Some(size) = lock(&self.sizes).get(p) {
                    return *size;
                }
                let size = ssg_images::probe_file(p).ok().map(|(s, _)| s);
                *lock(&self.sizes).entry(p.clone()).or_insert(size)
            }
            Body::Bytes(b) => ssg_images::probe(b, &r.name).ok().map(|(s, _)| s),
            Body::Pending => None,
        }
    }

    /// The resource of a queued image operation on `from` (its [`image_input`](Self::image_input)):
    /// the result's file name in the source's directory; name, title and params of the source.
    pub fn register_image(&self, from: ResourceId, e: &Enqueued) -> ResourceId {
        let src = self.resource(from);
        let sibling = |p: &str| paths::join(&["/", paths::dir(p), &e.file_name]);
        let ext = e.format.extension().trim_start_matches('.');
        let media_type = self.cfg.media_types.by_suffix(ext).map_or_else(
            || self.media_type_of(&e.file_name),
            |id| self.cfg.media_types.get(id).clone(),
        );
        let result: Result<ResourceId, std::convert::Infallible> =
            self.images.get_or_try((from, e.id), || {
                Ok(self.push(NewResource {
                    origin: Origin::Image { from },
                    media_type,
                    name: src.name.clone(),
                    name_normalized: Some(src.name_normalized.clone()),
                    title: src.title.clone(),
                    params: src.params.clone(),
                    data: src.data.clone(),
                    lang: src.lang,
                    target: OutputPath::new(&sibling(src.target.as_str())),
                    link: UrlPath::new(&sibling(src.link.as_str())),
                    body: Body::PendingImage(e.id),
                    policy: PublishPolicy::OnReference,
                    kind: Some(ResourceKind::Image),
                }))
            });
        match result {
            Ok(id) => id,
            Err(never) => match never {},
        }
    }

    /// `.Content`: the resource's bytes (reading the file, or processing the image).
    ///
    /// # Errors
    /// An unreadable file, or an image that cannot be processed.
    pub fn content(&self, id: ResourceId) -> Result<Arc<[u8]>, ResourceError> {
        let r = self.resource(id);
        match &r.body {
            Body::File(p) => std::fs::read(p)
                .map(Into::into)
                .map_err(|e| ResourceError::io(p, e)),
            Body::Bytes(b) => Ok(Arc::clone(b)),
            Body::PendingImage(op) => match &self.cfg.images {
                Some(q) => Ok(q.encoded(*op)?),
                None => Err(ResourceError::NotAnImage(r.name.clone())),
            },
            Body::Pending => {
                self.realize(id)?;
                self.content(id)
            }
        }
    }

    /// The `publish` filter: `id` is published even if no output references it (unless its
    /// policy is [`PublishPolicy::Never`]).
    pub fn mark_published(&self, id: ResourceId) {
        lock(&self.marked).insert(id);
    }
}
