//! The resource arena, its identities and the resource factories.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::hash::Hash;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use md5::Md5;
use sha2::{Digest, Sha256, Sha384, Sha512};
use ssg_base::diag::Position;
use ssg_base::glob::{self, GlobError, GlobOpts};
use ssg_base::paths::{self, OutputPath, Permalink, UrlPath};
use ssg_base::url::BaseUrl;
use ssg_base::{IdVec, Idx, ImageOpId, LangIdx, Map, PageId, Params, ResourceId, Value};
use ssg_config::{Config, MediaType, MediaTypes};
use ssg_images::{Enqueued, ImageError, ImageFormat, ImageInput, ImageQueue, QrLevel};
use ssg_vfs::{Component, Vfs, VfsError};
use xxhash_rust::xxh3::xxh3_64;

use crate::gohash;
use crate::pipes::{self, PipeError, PipeState, Transform, TransformEnv};
use crate::remote::{RemoteConfig, RemoteState};

mod assets;
mod create;
mod kinds;
mod transform;

pub use kinds::*;

/// A resource: immutable once registered.
#[derive(Clone, Debug)]
pub struct Resource {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub origin: Origin,
    /// The media type; an unknown extension gives an empty one (`main` and `sub` empty).
    pub media_type: MediaType,
    /// `.Name`: the path relative to the bundle (`sub/deep.txt`), or `/`-rooted for global
    /// resources (`/css/a.css`).
    pub name: String,
    /// The name lower-cased with spaces as dashes (`Pic 2.JPG` → `pic-2.jpg`).
    pub name_normalized: String,
    pub title: String,
    pub params: Params,
    /// `.Data`: `Integrity` of fingerprinted resources, the response data of remote ones.
    pub data: Map,
    /// The language whose base URL the links use.
    pub lang: LangIdx,
    /// The file under the publish directory.
    pub target: OutputPath,
    /// The site-relative link path, unescaped and without the base path.
    pub link: UrlPath,
    /// The escaped link with the base URL's path (`/sub/a%20b/c.txt`).
    pub rel_permalink: String,
    pub permalink: Permalink,
    pub body: Body,
    pub policy: PublishPolicy,
}

impl Resource {
    /// `.ResourceType`: the main type of the media type (`image`, `text`, `application`), or
    /// `page`.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        match self.kind {
            ResourceKind::Page(_) => "page",
            _ => &self.media_type.main,
        }
    }

    /// The media type as a string (`text/css`; empty when unknown).
    #[must_use]
    pub fn media_type_string(&self) -> String {
        if self.media_type.main.is_empty() {
            String::new()
        } else {
            self.media_type.to_string()
        }
    }

    /// `Data.Integrity` (fingerprinted resources).
    #[must_use]
    pub fn integrity(&self) -> Option<&str> {
        self.data.get("Integrity").and_then(Value::as_str)
    }
}

/// Why a resource could not be made, read or published.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error(transparent)]
    Glob(#[from] GlobError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error("unsupported hash algorithm {0:?}: use md5, sha256, sha384 or sha512")]
    UnsupportedHash(String),
    /// Two calls in one language made different resources for one target path.
    #[error(
        "{target} is created twice in one language with different inputs: at {first} and at {second}"
    )]
    TargetConflict {
        target: OutputPath,
        first: CallSite,
        second: CallSite,
    },
    #[error(
        "resources to concatenate must have one media type: {first} is {first_type:?}, {other} is {other_type:?}"
    )]
    MixedMediaTypes {
        first: String,
        first_type: String,
        other: String,
        other_type: String,
    },
    #[error("an empty target path")]
    EmptyTarget,
    #[error("resource metadata entry {index}: {reason}")]
    Metadata { index: usize, reason: String },
    #[error("{0}: the image has no pixels to read (not a file or a processed image)")]
    NotAnImage(String),
    #[error("writing {path}: {source}")]
    Write { path: OutputPath, source: io::Error },
    /// A transform (`to_css`, `js_build`, …) failed.
    #[error("{resource}: {transform}: {source}")]
    Pipe {
        resource: String,
        transform: &'static str,
        source: Box<PipeError>,
    },
}

impl ResourceError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// A language's URLs.
#[derive(Clone, Debug)]
pub struct LangTarget {
    pub base_url: BaseUrl,
    /// What goes before a global resource's target: `/<lang>` on multihost sites, else empty.
    pub target_prefix: String,
}

/// What a store needs from the configuration and the other build services.
#[derive(Clone)]
pub struct StoreConfig {
    /// Per language, in language order (at least one).
    pub languages: Vec<LangTarget>,
    /// Whether each language has its own host: global resources then exist per language.
    pub multihost: bool,
    pub media_types: Arc<MediaTypes>,
    /// The union file views; `None` gives a store without assets.
    pub vfs: Option<Arc<Vfs>>,
    /// Processed images; `None` gives a store that cannot publish or read them.
    pub images: Option<Arc<ImageQueue>>,
    pub remote: RemoteConfig,
    /// What the pipes need: the project and publish directories, external tools, the
    /// `js_build` bundler, the minifier.
    pub transforms: Arc<TransformEnv>,
}

impl StoreConfig {
    /// The store configuration of a loaded project.
    #[must_use]
    pub fn from_config(
        cfg: &Config,
        vfs: Option<Arc<Vfs>>,
        images: Option<Arc<ImageQueue>>,
    ) -> Self {
        let languages = cfg
            .sites
            .iter()
            .map(|s| LangTarget {
                base_url: s.base_url.clone(),
                target_prefix: if cfg.multihost {
                    format!("/{}", s.language.key)
                } else {
                    String::new()
                },
            })
            .collect();
        Self {
            languages,
            multihost: cfg.multihost,
            media_types: Arc::clone(&cfg.media_types),
            vfs,
            images,
            remote: RemoteConfig::from_config(cfg),
            transforms: Arc::new(TransformEnv::from_config(cfg)),
        }
    }
}

/// A bundle file to register (see [`ResourceStore::register_bundle`]).
#[derive(Clone, Debug)]
pub struct BundleResource {
    pub lang: LangIdx,
    pub file: PathBuf,
    /// The path relative to the bundle directory, `/`-separated (`sub/deep.txt`).
    pub name: String,
    /// The owning page's link directory, unescaped (`/blog/bundle1`, `/fr/blog/b`).
    pub dir: String,
    pub policy: PublishPolicy,
}

/// A page resource a content adapter added (`add_resource`; Go's `ResourceConfig`).
#[derive(Clone, Debug)]
pub struct AdapterResource {
    pub lang: LangIdx,
    /// Its bytes: text the adapter gave, or the body of the resource it passed.
    pub body: Body,
    /// `content.mediaType`, else the media type of the resource it passed (`None`: from the
    /// name's extension).
    pub media_type: Option<String>,
    /// The path below the owning page (`sub/data.yaml`) and the page's link directory, as for
    /// a bundle file ([`BundleResource`]).
    pub name: String,
    pub dir: String,
    /// A resource the adapter passed keeps its own file and link (Go publishes it relative
    /// to the site root); `None`: below the page.
    pub place: Option<(OutputPath, UrlPath)>,
    /// `name`, `title` and `params` of the map (`None`: the name below the page, the name).
    pub display_name: Option<String>,
    pub title: Option<String>,
    pub params: Params,
    pub policy: PublishPolicy,
}

/// One memoized construction per key; failures are not remembered.
pub(crate) struct Memo<K>(Mutex<HashMap<K, Arc<Mutex<Option<ResourceId>>>>>);

impl<K: Hash + Eq> Memo<K> {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub(crate) fn get_or_try<E>(
        &self,
        key: K,
        f: impl FnOnce() -> Result<ResourceId, E>,
    ) -> Result<ResourceId, E> {
        let cell = Arc::clone(lock(&self.0).entry(key).or_default());
        let mut slot = lock(&cell);
        if let Some(id) = *slot {
            return Ok(id);
        }
        let id = f()?;
        *slot = Some(id);
        Ok(id)
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The first claim of a target path.
struct Claim {
    id: ResourceId,
    input: u64,
    call: CallSite,
}

/// The fields of a resource being made; the store adds id and links.
pub(crate) struct NewResource {
    pub(crate) origin: Origin,
    pub(crate) media_type: MediaType,
    pub(crate) name: String,
    /// `None`: [`normalize_name`] of `name`.
    pub(crate) name_normalized: Option<String>,
    pub(crate) title: String,
    pub(crate) params: Params,
    pub(crate) data: Map,
    pub(crate) lang: LangIdx,
    pub(crate) target: OutputPath,
    pub(crate) link: UrlPath,
    pub(crate) body: Body,
    pub(crate) policy: PublishPolicy,
    pub(crate) kind: Option<ResourceKind>,
}

/// Every resource of a build (see the crate documentation).
pub struct ResourceStore {
    pub(crate) cfg: StoreConfig,
    arena: RwLock<IdVec<ResourceId, Arc<Resource>>>,
    assets: Memo<(LangIdx, String)>,
    asset_files: Mutex<Option<Arc<Vec<String>>>>,
    bundles: Memo<OutputPath>,
    transforms: Memo<(ResourceId, Transform)>,
    metas: Memo<(ResourceId, String, String, String)>,
    images: Memo<(ResourceId, ImageOpId)>,
    /// Header sizes of images that are not processed, per source file.
    sizes: Mutex<BTreeMap<PathBuf, Option<(u32, u32)>>>,
    targets: Mutex<BTreeMap<OutputPath, Claim>>,
    pub(crate) marked: Mutex<BTreeSet<ResourceId>>,
    /// The results of `execute_as_template`: template output whose URLs publish resources.
    template_outputs: Mutex<BTreeSet<ResourceId>>,
    pub(crate) published: Mutex<BTreeSet<OutputPath>>,
    pub(crate) remote: RemoteState,
    pub(crate) pipes: PipeState,
}

impl ResourceStore {
    /// An empty store.
    ///
    /// # Panics
    /// When `cfg.languages` is empty.
    #[must_use]
    pub fn new(cfg: StoreConfig) -> Self {
        assert!(!cfg.languages.is_empty(), "a store needs a language");
        Self {
            cfg,
            arena: RwLock::new(IdVec::default()),
            assets: Memo::new(),
            asset_files: Mutex::new(None),
            bundles: Memo::new(),
            transforms: Memo::new(),
            metas: Memo::new(),
            images: Memo::new(),
            sizes: Mutex::new(BTreeMap::new()),
            targets: Mutex::new(BTreeMap::new()),
            marked: Mutex::new(BTreeSet::new()),
            template_outputs: Mutex::new(BTreeSet::new()),
            published: Mutex::new(BTreeSet::new()),
            remote: RemoteState::default(),
            pipes: PipeState::default(),
        }
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> &StoreConfig {
        &self.cfg
    }

    /// The resource with this id.
    ///
    /// # Panics
    /// When the id was not handed out by this store.
    #[must_use]
    pub fn resource(&self, id: ResourceId) -> Arc<Resource> {
        Arc::clone(&self.arena.read().unwrap_or_else(PoisonError::into_inner)[id])
    }

    /// Every resource, in id order.
    #[must_use]
    pub fn resources(&self) -> Vec<Arc<Resource>> {
        self.arena
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// The number of resources.
    #[must_use]
    pub fn len(&self) -> usize {
        self.arena
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lang_target(&self, lang: LangIdx) -> &LangTarget {
        self.cfg
            .languages
            .get(lang.index())
            .unwrap_or(&self.cfg.languages[0])
    }

    /// The language whose URLs global resources of `lang` use: `lang` on multihost sites,
    /// else the first language.
    pub(crate) fn global_lang(&self, lang: LangIdx) -> LangIdx {
        if self.cfg.multihost {
            lang
        } else {
            LangIdx::from_index(0)
        }
    }

    /// The target of a global resource at `link` (`/css/a.css`) in `lang`.
    pub(crate) fn global_target(&self, lang: LangIdx, link: &str) -> OutputPath {
        OutputPath::new(&format!(
            "{}{link}",
            self.lang_target(self.global_lang(lang)).target_prefix
        ))
    }

    /// The escaped relative permalink of `link` in `lang` (with the base URL's path).
    pub(crate) fn rel_permalink(&self, lang: LangIdx, link: &UrlPath) -> String {
        let base_path = self
            .lang_target(lang)
            .base_url
            .base_path_no_trailing_slash();
        format!("{base_path}{}", link.escaped())
    }

    /// The permalink of `link` in `lang`.
    pub(crate) fn permalink(&self, lang: LangIdx, link: &UrlPath) -> Permalink {
        Permalink::new(&self.lang_target(lang).base_url, link)
    }

    pub(crate) fn push(&self, n: NewResource) -> ResourceId {
        let rel_permalink = self.rel_permalink(n.lang, &n.link);
        let permalink = self.permalink(n.lang, &n.link);
        let kind = n.kind.unwrap_or_else(|| kind_of(&n.media_type));
        let mut arena = self.arena.write().unwrap_or_else(PoisonError::into_inner);
        let id = arena.next_id();
        arena.push(Arc::new(Resource {
            id,
            kind,
            origin: n.origin,
            name_normalized: n.name_normalized.unwrap_or_else(|| normalize_name(&n.name)),
            media_type: n.media_type,
            name: n.name,
            title: n.title,
            params: n.params,
            data: n.data,
            lang: n.lang,
            target: n.target,
            link: n.link,
            rel_permalink,
            permalink,
            body: n.body,
            policy: n.policy,
        }));
        id
    }

    /// The media type of a file name: the configured type of its extension (`xml` is
    /// `application/xml`), else the well-known type of the extension, else an empty type;
    /// `application/octet-stream` without extension.
    #[must_use]
    pub fn media_type_of(&self, name: &str) -> MediaType {
        let types = &self.cfg.media_types;
        let ext = paths::ext_no_delimiter(name).to_ascii_lowercase();
        let by_type = |t: &str| types.by_type(t).map(|id| types.get(id).clone());
        if ext.is_empty() {
            return by_type("application/octet-stream").unwrap_or_else(empty_media_type);
        }
        let configured = if ext == "xml" {
            by_type("application/xml")
        } else {
            types.by_suffix(&ext).map(|id| types.get(id).clone())
        };
        configured.unwrap_or_else(|| well_known_media_type(&ext).unwrap_or_else(empty_media_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(add_identifier("/a/b.css", ".min"), "/a/b.min.css");
        assert_eq!(add_identifier("/a/noext", ".1"), "/a/noext.1");
        assert_eq!(add_identifier("/a/x.1.y", ".2"), "/a/x.1.2.y");
        assert_eq!(normalize_name("/A b/Pic 2.JPG"), "/a-b/pic-2.jpg");
    }
}
