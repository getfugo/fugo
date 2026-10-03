//! `resources.PostProcess`: the fields of a resource as placeholders, filled in build phase E5
//! (REWRITE_PLAN.md §3.4).
//!
//! A post-processed resource gets a [`PostProcessId`] (per resource, from 1); each of its
//! fields ([`PpField`]) is written as `__nh_pp_<id>_<field>__`. Outputs holding placeholders
//! are held; in E5, after every page is rendered, [`ResourceStore::resolve_post_process`]
//! computes the resource (its pending transforms run now) and replaces the placeholders with
//! the field values (verbatim: content is not escaped, links are final).

use std::collections::HashMap;
use std::sync::Mutex;

use ssg_base::ResourceId;

use crate::store::{ResourceError, ResourceStore, lock};

const PREFIX: &str = "__nh_pp_";

/// A post-processed resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PostProcessId(u32);

impl PostProcessId {
    /// The number in the placeholders (from 1).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The placeholder of `field`: `__nh_pp_<id>_<field>__`.
    #[must_use]
    pub fn placeholder(self, field: PpField) -> String {
        format!("{PREFIX}{}_{}__", self.0, field.key())
    }
}

/// A field of a post-processed resource that is written as a placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PpField {
    /// `.Content` (text, verbatim).
    Content,
    RelPermalink,
    Permalink,
    /// `.Data.Integrity` (empty without a fingerprint).
    Integrity,
    /// `.MediaType` as a string (`text/css`).
    MediaType,
}

impl PpField {
    /// Every field.
    pub const ALL: [Self; 5] = [
        Self::Content,
        Self::RelPermalink,
        Self::Permalink,
        Self::Integrity,
        Self::MediaType,
    ];

    /// The field's name in placeholders.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::RelPermalink => "rel_permalink",
            Self::Permalink => "permalink",
            Self::Integrity => "integrity",
            Self::MediaType => "media_type",
        }
    }
}

/// The post-processed resources of a store.
#[derive(Default)]
pub(crate) struct Registry(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    ids: HashMap<ResourceId, PostProcessId>,
    resources: Vec<ResourceId>,
}

/// Whether `text` holds a post-process placeholder (the publisher holds such outputs).
#[must_use]
pub fn has_placeholder(text: &str) -> bool {
    text.contains(PREFIX)
}

/// The placeholder at the start of `s` (after the prefix): its id, field and length.
fn parse(s: &str) -> Option<(u32, PpField, usize)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let n: u32 = s[..digits].parse().ok()?;
    let rest = s[digits..].strip_prefix('_')?;
    PpField::ALL.into_iter().find_map(|f| {
        let after = rest.strip_prefix(f.key())?.strip_prefix("__")?;
        Some((n, f, s.len() - after.len()))
    })
}

impl ResourceStore {
    /// `resources.PostProcess`: the post-process id of `id` (the same for every call).
    pub fn post_process(&self, id: ResourceId) -> PostProcessId {
        let mut inner = lock(&self.pipes.post.0);
        if let Some(pp) = inner.ids.get(&id) {
            return *pp;
        }
        let pp = PostProcessId(u32::try_from(inner.resources.len() + 1).unwrap_or(u32::MAX));
        inner.resources.push(id);
        inner.ids.insert(id, pp);
        pp
    }

    /// Every post-process id handed out so far, in order (phase E5 resolves their
    /// placeholders).
    #[must_use]
    pub fn post_processes(&self) -> Vec<PostProcessId> {
        let n = lock(&self.pipes.post.0).resources.len();
        (1..=n)
            .map(|i| PostProcessId(u32::try_from(i).unwrap_or(u32::MAX)))
            .collect()
    }

    /// The resource a post-process id stands for.
    #[must_use]
    pub fn post_processed(&self, pp: PostProcessId) -> Option<ResourceId> {
        let i = usize::try_from(pp.0).ok()?.checked_sub(1)?;
        lock(&self.pipes.post.0).resources.get(i).copied()
    }

    /// Replaces the post-process placeholders in `text` by the field values, computing the
    /// resources; `None` when there is no placeholder. Unknown placeholders stay.
    ///
    /// # Errors
    /// A failing transform of a post-processed resource.
    pub fn resolve_post_process(&self, text: &str) -> Result<Option<String>, ResourceError> {
        if !has_placeholder(text) {
            return Ok(None);
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find(PREFIX) {
            out.push_str(&rest[..i]);
            let after = &rest[i + PREFIX.len()..];
            let found = parse(after)
                .and_then(|(n, f, len)| Some((self.post_processed(PostProcessId(n))?, f, len)));
            let Some((id, field, len)) = found else {
                out.push_str(PREFIX);
                rest = after;
                continue;
            };
            let r = self.realize(id)?;
            match field {
                PpField::Content => out.push_str(&String::from_utf8_lossy(&self.content(id)?)),
                PpField::RelPermalink => out.push_str(&r.rel_permalink),
                PpField::Permalink => out.push_str(r.permalink.as_str()),
                PpField::Integrity => out.push_str(r.integrity().unwrap_or_default()),
                PpField::MediaType => out.push_str(&r.media_type_string()),
            }
            rest = &after[len..];
        }
        out.push_str(rest);
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders() {
        let pp = PostProcessId(12);
        assert_eq!(
            pp.placeholder(PpField::RelPermalink),
            "__nh_pp_12_rel_permalink__"
        );
        for f in PpField::ALL {
            let p = pp.placeholder(f);
            assert_eq!(
                parse(&p[PREFIX.len()..]),
                Some((12, f, p.len() - PREFIX.len()))
            );
        }
        assert_eq!(parse("3_nope__"), None);
        assert_eq!(parse("x_content__"), None);
    }
}
