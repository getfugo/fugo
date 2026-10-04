//! Making resources: claimed names, strings, template output, concatenation, copies and QR codes.

use super::*;

impl ResourceStore {
    /// Claims `target` for a resource made from `input` (a hash of the inputs): the first claim
    /// makes it; a claim in another language gets the first resource; a claim in the same
    /// language with other inputs is an error.
    pub(super) fn claim(
        &self,
        target: &str,
        input: u64,
        call: &CallSite,
        make: impl FnOnce(OutputPath, String) -> Result<NewResource, ResourceError>,
    ) -> Result<ResourceId, ResourceError> {
        let link = paths::clean(&format!("/{target}"));
        if link == "/" {
            return Err(ResourceError::EmptyTarget);
        }
        let out = self.global_target(call.lang, &link);
        let mut targets = lock(&self.targets);
        if let Some(c) = targets.get(&out) {
            if c.input == input || c.call.lang != call.lang {
                return Ok(c.id);
            }
            return Err(ResourceError::TargetConflict {
                target: out,
                first: c.call.clone(),
                second: call.clone(),
            });
        }
        let id = self.push(make(out.clone(), link)?);
        targets.insert(
            out,
            Claim {
                id,
                input,
                call: call.clone(),
            },
        );
        Ok(id)
    }

    pub(super) fn named(
        &self,
        lang: LangIdx,
        target: OutputPath,
        link: String,
        body: Body,
    ) -> NewResource {
        NewResource {
            origin: Origin::Named,
            media_type: self.media_type_of(&link),
            name: link.clone(),
            name_normalized: Some(link.clone()),
            link: UrlPath::new(&link),
            title: link,
            params: Params::default(),
            data: Map::new(),
            lang: self.global_lang(lang),
            target,
            body,
            policy: PublishPolicy::OnReference,
            kind: None,
        }
    }

    /// `resources.FromString`.
    ///
    /// # Errors
    /// An empty target, or a conflicting earlier call in the same language.
    pub fn from_string(
        &self,
        target: &str,
        content: &str,
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let input = xxh3_64(content.as_bytes());
        self.claim(target, input, call, |out, link| {
            Ok(self.named(call.lang, out, link, Body::Bytes(content.as_bytes().into())))
        })
    }

    /// The output of a template executed for a target path (`resources.ExecuteAsTemplate`
    /// after rendering).
    ///
    /// # Errors
    /// As [`from_string`](Self::from_string).
    pub fn from_template_output(
        &self,
        target: &str,
        output: String,
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let input = xxh3_64(output.as_bytes()) ^ 0x7465_6d70_6c61_7465;
        let id = self.claim(target, input, call, |out, link| {
            Ok(self.named(
                call.lang,
                out,
                link,
                Body::Bytes(output.into_bytes().into()),
            ))
        })?;
        lock(&self.template_outputs).insert(id);
        Ok(id)
    }

    /// The resources made by [`from_template_output`](Self::from_template_output), in id
    /// order: their text is template output, so the build extracts URL tokens from it
    /// (REWRITE_PLAN.md §3.4).
    #[must_use]
    pub fn template_outputs(&self) -> Vec<ResourceId> {
        lock(&self.template_outputs).iter().copied().collect()
    }

    /// `resources.Concat`: the items' contents joined (JavaScript parts with `\n;\n` between
    /// them); the media type comes from the target.
    ///
    /// # Errors
    /// Items of different media types, an unreadable item, or a target conflict.
    pub fn concat(
        &self,
        target: &str,
        items: &[ResourceId],
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let resources: Vec<Arc<Resource>> = items.iter().map(|&id| self.resource(id)).collect();
        if let Some(first) = resources.first()
            && let Some(other) = resources.iter().find(|r| r.media_type != first.media_type)
        {
            return Err(ResourceError::MixedMediaTypes {
                first: first.name.clone(),
                first_type: first.media_type_string(),
                other: other.name.clone(),
                other_type: other.media_type_string(),
            });
        }
        let mut key = Vec::with_capacity(items.len() * 4);
        for id in items {
            key.extend_from_slice(&id.raw().to_le_bytes());
        }
        let input = xxh3_64(&key);
        self.claim(target, input, call, |out, link| {
            let js = resources
                .first()
                .is_some_and(|r| r.media_type.main == "text" && r.media_type.sub == "javascript");
            let mut bytes = Vec::new();
            for (i, r) in resources.iter().enumerate() {
                if i > 0 && js {
                    bytes.extend_from_slice(b"\n;\n");
                }
                bytes.extend_from_slice(&self.content(r.id)?);
            }
            Ok(self.named(call.lang, out, link, Body::Bytes(bytes.into())))
        })
    }

    /// `resources.Copy`: `id` published at `target` as well (media type, name, title, params
    /// and data unchanged).
    ///
    /// # Errors
    /// An empty target, or a conflicting earlier call in the same language.
    pub fn copy(
        &self,
        target: &str,
        id: ResourceId,
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let src = self.resource(id);
        let input = u64::from(id.raw()) ^ 0x636f_7079_0000_0000;
        self.claim(target, input, call, |out, link| {
            Ok(NewResource {
                origin: Origin::Named,
                media_type: src.media_type.clone(),
                name: src.name.clone(),
                name_normalized: Some(src.name_normalized.clone()),
                title: src.title.clone(),
                params: src.params.clone(),
                data: src.data.clone(),
                lang: self.global_lang(call.lang),
                target: out,
                link: UrlPath::new(&link),
                body: src.body.clone(),
                policy: PublishPolicy::OnReference,
                kind: Some(src.kind),
            })
        })
    }

    /// `images.QR`: the PNG of the QR code of `text` (see [`ssg_images::qr_png`], equal to
    /// Go's bytes), published at [`qr_target`] — Go's name, so its URLs are the Go build's.
    ///
    /// # Errors
    /// Empty or too long text, a scale below 2, or (never in practice: the name hashes the
    /// inputs) a target conflict.
    pub fn qr_code(
        &self,
        text: &str,
        options: &QrOptions,
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let target = qr_target(text, options);
        // The name hashes every input: the same name is the same image.
        let input = xxh3_64(target.as_bytes());
        self.claim(&target, input, call, |out, link| {
            let png = ssg_images::qr_png(text, options.level, options.scale)?;
            Ok(self.named(call.lang, out, link, Body::Bytes(png.into())))
        })
    }
}
