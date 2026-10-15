//! Credits: the copyright notice and author an image resource gives itself ([`Credit`]),
//! written into it as published and into every image made from it, in place of `[imaging]`'s.

use super::*;

impl ResourceStore {
    /// The credit image resource `id` gives itself in its params, its `copyright` and `author`
    /// ([`ImageMetadata::credit`]: none without `stripMetadata`); `None` for other resources.
    ///
    /// # Errors
    /// [`ResourceError::Credit`]: a `copyright` or `author` param that cannot be written into
    /// an image.
    pub fn params_credit(&self, id: ResourceId) -> Result<Option<Credit>, ResourceError> {
        let r = self.resource(id);
        if r.kind != ResourceKind::Image {
            return Ok(None);
        }
        Ok(self.cfg.image_metadata.credit(&r.params)?)
    }

    /// Writes `credit` into file `id` (a bundle file or an adapter's resource) as published,
    /// and into every image made from it (its metadata views, transforms, copies and processed
    /// images), in place of `[imaging]`'s copyright and author. The views choose one credit for
    /// all the resources read from one source, which several pages may publish (a bundle file
    /// and the copies of it that translations get, the resources adapter runs add from one
    /// file: `ssg_view::page_resources`), and give it before any render.
    pub fn set_credit(&self, id: ResourceId, credit: Credit) {
        lock(&self.credits).insert(id, credit);
    }

    /// The credit resource `id` is published with: that of the file it is made from
    /// ([`set_credit`](Self::set_credit)); `None`: `[imaging]`'s.
    #[must_use]
    pub fn credit(&self, id: ResourceId) -> Option<Credit> {
        let file = self.file_of(id);
        lock(&self.credits).get(&file).cloned()
    }

    /// The credit of what is published at one target, of `candidates` (the resources at that
    /// target): of the files they are made from, that of the first, by id, that has one. Files
    /// get their credits before any render, so the choice does not depend on the order of the
    /// renders that registered the candidates (the result of one operation on two views of a
    /// file is registered twice, in either order).
    pub(crate) fn credit_at(&self, candidates: &[&Resource]) -> Option<Credit> {
        let files: BTreeSet<ResourceId> = candidates.iter().map(|r| self.file_of(r.id)).collect();
        let credits = lock(&self.credits);
        files.iter().find_map(|f| credits.get(f)).cloned()
    }

    /// `copy`, made by `resources.Copy` of `id` (the same image at another path), carries
    /// `id`'s credit.
    pub(super) fn copy_credit(&self, id: ResourceId, copy: ResourceId) {
        if let Some(credit) = self.credit(id) {
            lock(&self.credits).insert(copy, credit);
        }
    }

    /// The resource `id` is made from: the end of its chain of sources by metadata views,
    /// transforms and image processing (`id` itself when it has none).
    fn file_of(&self, id: ResourceId) -> ResourceId {
        let mut at = id;
        loop {
            let (Origin::Meta { from } | Origin::Transformed { from, .. } | Origin::Image { from }) =
                self.resource(at).origin
            else {
                return at;
            };
            // Sources are registered before what is made from them; anything else is not a
            // chain.
            if from >= at {
                return at;
            }
            at = from;
        }
    }
}
