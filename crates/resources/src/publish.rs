//! Publishing: eager bundle resources, and every other resource whose URL the rendered outputs
//! reference (REWRITE_PLAN.md §3.4).
//!
//! The publisher extracts URL tokens from every output; this module resolves them against the
//! permalink and the relative permalink of every resource. A token may come in any of the
//! forms a URL takes in HTML, JSON or CSS: raw (`/b/Lay's.jpg`), HTML-escaped
//! (`/b/Lay&#39;s.jpg`, `&amp;`), JSON-escaped (`&`, `\/`), percent-encoded in upper or
//! lower case (`/b/Lay%27s.jpg`), with a query or fragment, absolute, or protocol-relative.
//! All of them are reduced to one canonical form: the decoded path, plus the scheme-less host
//! for absolute URLs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use ssg_base::paths::OutputPath;
use ssg_base::url::{Component, unescape};
use ssg_base::{Idx as _, ImageOpId, ResourceId, Sink};

use crate::store::{Body, PublishPolicy, Resource, ResourceError, ResourceStore, lock};

/// What [`ResourceStore::publish`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublishStats {
    /// Files written by this call (processed images included).
    pub files: usize,
    /// Processed images among them.
    pub images: usize,
    /// Tokens that resolved to a resource.
    pub resolved: usize,
}

/// Decodes the HTML character references a URL attribute can carry (`&amp;`, `&#39;`,
/// `&#x27;`, `&quot;`, `&apos;`, `&lt;`, `&gt;`); anything else stays.
fn decode_html(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "apos" => Some('\''),
                "quot" => Some('"'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                _ => entity.strip_prefix('#').and_then(|n| {
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse().ok(),
                    };
                    code.and_then(char::from_u32)
                }),
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decodes the JSON string escapes a URL can carry (`\/`, `\uXXXX`, `\\`).
fn decode_json(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(e @ ('/' | '\\' | '"' | '\'')) => {
                out.push(e);
                chars.next();
            }
            Some('u') => {
                let hex: String = chars.clone().skip(1).take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(d) if hex.len() == 4 => {
                        out.push(d);
                        for _ in 0..5 {
                            chars.next();
                        }
                    }
                    _ => out.push('\\'),
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}

/// The canonical form of a URL token: `//host/decoded/path` for absolute and
/// protocol-relative URLs, `/decoded/path` for rooted ones; `None` for anything else.
pub(crate) fn canonical(token: &str) -> Option<String> {
    let t = decode_json(&decode_html(token.trim()));
    let t = t.split(['?', '#']).next().unwrap_or_default();
    let rest = match t.find("://") {
        Some(i)
            if i > 0
                && t[..i]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) =>
        {
            format!("//{}", &t[i + 3..])
        }
        _ if t.starts_with('/') => t.to_owned(),
        _ => return None,
    };
    let (host, path) = match rest.strip_prefix("//") {
        Some(r) => {
            let slash = r.find('/').unwrap_or(r.len());
            (Some(r[..slash].to_ascii_lowercase()), &r[slash..])
        }
        None => (None, rest.as_str()),
    };
    let path = match unescape(path, Component::Path) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| path.to_owned()),
        Err(_) => path.to_owned(),
    };
    let path = if path.is_empty() {
        "/".to_owned()
    } else {
        path
    };
    Some(match host {
        Some(h) => format!("//{h}{path}"),
        None => path,
    })
}

impl ResourceStore {
    /// The canonical URL forms of every resource: relative and absolute.
    fn url_index(resources: &[Arc<Resource>]) -> HashMap<String, Vec<ResourceId>> {
        let mut index: HashMap<String, Vec<ResourceId>> = HashMap::new();
        for r in resources {
            for u in [r.rel_permalink.as_str(), r.permalink.as_str()] {
                if let Some(c) = canonical(u) {
                    index.entry(c).or_default().push(r.id);
                }
            }
        }
        index
    }

    /// The resources a URL token names (several when they share a target).
    #[must_use]
    pub fn resolve_token(&self, token: &str) -> Vec<ResourceId> {
        let index = Self::url_index(&self.resources());
        canonical(token)
            .and_then(|c| index.get(&c).cloned())
            .unwrap_or_default()
    }

    /// Writes what the build references and has not been written yet: resources with
    /// [`PublishPolicy::Eager`], resources marked with
    /// [`mark_published`](Self::mark_published), and resources named by one of `tokens`
    /// (except [`PublishPolicy::Never`]). Targets are written once, in path order; processed
    /// images are produced by the [`ImageQueue`](ssg_images::ImageQueue) (in parallel).
    /// Call it outside any render; calling it again publishes only what is new.
    ///
    /// # Errors
    /// An unreadable resource, a failing image operation, or a failing sink.
    pub fn publish<'t>(
        &self,
        tokens: impl IntoIterator<Item = &'t str>,
        sink: &dyn Sink,
    ) -> Result<PublishStats, ResourceError> {
        let resources = self.resources();
        let index = Self::url_index(&resources);
        let mut wanted: BTreeSet<ResourceId> = lock(&self.marked).iter().copied().collect();
        let mut stats = PublishStats::default();
        for t in tokens {
            if let Some(ids) = canonical(t).and_then(|c| index.get(&c)) {
                stats.resolved += 1;
                wanted.extend(ids.iter().copied());
            }
        }
        wanted.extend(
            resources
                .iter()
                .filter(|r| r.policy == PublishPolicy::Eager)
                .map(|r| r.id),
        );
        // Files that go with a published resource (source maps of the pipes).
        let companions = self.pipes.companions_of(&wanted);
        wanted.extend(companions);

        // One writer per target: the lowest id.
        let mut by_target: BTreeMap<OutputPath, &Resource> = BTreeMap::new();
        for r in wanted.iter().filter_map(|&id| resources.get(id.index())) {
            if r.policy != PublishPolicy::Never {
                by_target.entry(r.target.clone()).or_insert(r);
            }
        }
        let mut published = lock(&self.published);
        let mut images: BTreeMap<OutputPath, ImageOpId> = BTreeMap::new();
        for (target, r) in by_target {
            if published.contains(&target) {
                continue;
            }
            if let Body::PendingImage(op) = r.body {
                images.insert(target, op);
                continue;
            }
            let bytes = self.content(r.id)?;
            sink.write(&target, &bytes)
                .map_err(|source| ResourceError::Write {
                    path: target.clone(),
                    source,
                })?;
            published.insert(target);
            stats.files += 1;
        }
        if !images.is_empty() {
            let queue = self.cfg.images.as_ref().ok_or_else(|| {
                ResourceError::NotAnImage(
                    images
                        .keys()
                        .next()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                )
            })?;
            queue.process(&images, sink)?;
            stats.images = images.len();
            stats.files += images.len();
            published.extend(images.into_keys());
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_forms() {
        let want = Some("/b/herrs-salt-&-vinegar/Lay's.jpg".to_owned());
        for t in [
            "/b/herrs-salt-&-vinegar/Lay's.jpg",
            "/b/herrs-salt-&amp;-vinegar/Lay&#39;s.jpg",
            "/b/herrs-salt-&#38;-vinegar/Lay&#x27;s.jpg",
            "\\/b\\/herrs-salt-\\u0026-vinegar\\/Lay\\u0027s.jpg",
            "/b/herrs-salt-%26-vinegar/Lay%27s.jpg?v=1#top",
            " /b/herrs-salt-&-vinegar/Lay's.jpg ",
        ] {
            assert_eq!(canonical(t), want, "{t}");
        }
        assert_eq!(
            canonical("HTTPS://Example.org/a%20b/%c3%bc.txt"),
            Some("//example.org/a b/ü.txt".to_owned())
        );
        assert_eq!(
            canonical("//example.org/x"),
            Some("//example.org/x".to_owned())
        );
        assert_eq!(canonical("images/x.png"), None);
        assert_eq!(canonical("mailto:x@y"), None);
    }
}
