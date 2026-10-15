//! Credits: the `copyright` and `author` params the pages give the images they list (their
//! `resources` metadata applied), one credit per file.
//!
//! A file is what its resources are read from: a bundle file's source, shared by the copies of
//! it that translations get (multihost sites, `duplicateResourceFiles`, a translation whose
//! content is not Markdown), or the file or bytes of the resource a content adapter passed,
//! shared by its runs for several languages. Every resource of a file carries the credit of
//! the first page that gives it one, in language order (the default content language first,
//! then by weight; the lowest page id first within a language): a page that gives none, or
//! comes later, never takes away another's, and the copies of a photo carry one notice.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use ssg_base::{LangIdx, PageId, ResourceId};
use ssg_resources::{Body, Credit, Resource, ResourceKind, ResourceStore};

/// What a resource is read from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    File(PathBuf),
    Bytes(Arc<[u8]>),
    /// Neither (a resource that is not a registered bundle file or adapter resource): itself
    /// alone.
    Itself(ResourceId),
}

impl Source {
    fn of(r: &Resource) -> Self {
        match &r.body {
            Body::File(path) => Self::File(path.clone()),
            Body::Bytes(bytes) => Self::Bytes(Arc::clone(bytes)),
            Body::PendingImage(_) | Body::Pending => Self::Itself(r.id),
        }
    }
}

/// The store resources of one file, and the first credit given to them in language order, with
/// its page's place in that order.
#[derive(Default)]
struct File {
    resources: BTreeSet<ResourceId>,
    credit: Option<((LangIdx, PageId), Credit)>,
}

/// The credits the pages give, gathered per file ([`add`](Self::add)) before any is given to
/// the store ([`give`](Self::give)).
#[derive(Default)]
pub(super) struct Credits(BTreeMap<Source, File>);

impl Credits {
    /// Records that `page` (its language, then its id) lists store resource `id`, its view of
    /// it giving `credit`. Resources that are not images have none.
    pub(super) fn add(
        &mut self,
        store: &ResourceStore,
        id: ResourceId,
        page: (LangIdx, PageId),
        credit: Option<Credit>,
    ) {
        let r = store.resource(id);
        if r.kind != ResourceKind::Image {
            return;
        }
        let file = self.0.entry(Source::of(&r)).or_default();
        file.resources.insert(id);
        if let Some(credit) = credit
            && file.credit.as_ref().is_none_or(|(first, _)| page < *first)
        {
            file.credit = Some((page, credit));
        }
    }

    /// Gives every store resource of each file the file's credit
    /// ([`ResourceStore::set_credit`]).
    pub(super) fn give(self, store: &ResourceStore) {
        for file in self.0.into_values() {
            if let Some((_, credit)) = file.credit {
                for id in file.resources {
                    store.set_credit(id, credit.clone());
                }
            }
        }
    }
}
