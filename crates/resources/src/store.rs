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

/// The options of `images.QR` (Go's defaults: medium, 4, no directory).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrOptions {
    pub level: QrLevel,
    /// Image pixels per module, at least 2.
    pub scale: u32,
    /// The directory of the image below the publish directory.
    pub target_dir: String,
}

impl Default for QrOptions {
    fn default() -> Self {
        Self {
            level: QrLevel::Medium,
            scale: 4,
            target_dir: String::new(),
        }
    }
}

/// Go's target path of `images.QR text options`: `<targetDir>/qr_<hash>.png`, the hash being
/// `hashing.HashStringHex(text, opts)` of the decoded options struct
/// `{Level string; Scale int; TargetDir string}` (hex without leading zeros).
#[must_use]
pub fn qr_target(text: &str, options: &QrOptions) -> String {
    let opts = gohash::structure(
        "",
        &[
            ("Level", gohash::string(options.level.name())),
            ("Scale", gohash::int(i64::from(options.scale))),
            ("TargetDir", gohash::string(&options.target_dir)),
        ],
    );
    let hash = gohash::list([gohash::string(text), opts]);
    paths::clean(&format!("/{}/qr_{hash:x}.png", options.target_dir))
}

/// What a resource is, for the template layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// A raster image the [`ImageQueue`] can process (JPEG, PNG, GIF, TIFF, BMP, WebP).
    Image,
    /// A page of a bundle (the site crate's; the store never creates one).
    Page(PageId),
    /// A text format (`text/*`, JSON, XML, SVG, JavaScript, TOML, YAML).
    Text,
    Other,
}

/// When a resource is written to the publish directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PublishPolicy {
    /// Always: bundle resources of rendered pages with `publishResources = true`.
    Eager,
    /// When its URL appears in a rendered output, or the `publish` filter marks it.
    OnReference,
    /// Never.
    Never,
}

/// Where a resource's bytes come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    File(PathBuf),
    Bytes(Arc<[u8]>),
    /// A processed image; the pixels come from the [`ImageQueue`].
    PendingImage(ImageOpId),
    /// A transform result not computed yet ([`Origin::Transformed`] names the source and the
    /// transform); [`ResourceStore::realize`] computes it.
    Pending,
}

/// How a resource was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `resources.Get`: the path inside the assets component (`css/a.css`).
    Asset { path: String },
    /// A bundle file of a page in `lang`.
    Bundle { lang: LangIdx },
    /// `resources.GetRemote`.
    Remote { url: String },
    /// `resources.FromString`, `resources.Concat`, `resources.Copy` or a template output.
    Named,
    /// A transform of another resource.
    Transformed {
        from: ResourceId,
        transform: Box<Transform>,
    },
    /// Another resource with front matter metadata (name, title, params) applied.
    Meta { from: ResourceId },
    /// A processed image of another resource.
    Image { from: ResourceId },
}

/// A hash algorithm of `fingerprint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HashAlgo {
    Md5,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgo {
    /// The name used in `integrity` values (`sha256`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
        }
    }

    pub(crate) fn digest(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => Md5::digest(bytes).to_vec(),
            Self::Sha256 => Sha256::digest(bytes).to_vec(),
            Self::Sha384 => Sha384::digest(bytes).to_vec(),
            Self::Sha512 => Sha512::digest(bytes).to_vec(),
        }
    }
}

impl std::str::FromStr for HashAlgo {
    type Err = ResourceError;

    /// `md5`, `sha256` (also the empty string), `sha384` or `sha512`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "" | "sha256" => Ok(Self::Sha256),
            "md5" => Ok(Self::Md5),
            "sha384" => Ok(Self::Sha384),
            "sha512" => Ok(Self::Sha512),
            other => Err(ResourceError::UnsupportedHash(other.to_owned())),
        }
    }
}

/// Where a call that names a target path was made: its language (sub-wave) and template
/// position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub lang: LangIdx,
    pub position: Option<Position>,
}

impl CallSite {
    /// A call in `lang` at an unknown position.
    #[must_use]
    pub const fn in_lang(lang: LangIdx) -> Self {
        Self {
            lang,
            position: None,
        }
    }
}

impl fmt::Display for CallSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.position {
            Some(p) => write!(f, "{p}"),
            None => f.write_str("<unknown position>"),
        }
    }
}

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

    // ── assets ──────────────────────────────────────────────────────────────────────────────

    /// `resources.Get`: the asset at `path` (cleaned; a leading `/` is ignored) in `lang`'s
    /// view, or `None` when there is no such file. Bytes injected with
    /// [`inject_generated`](Self::inject_generated) count as a file.
    ///
    /// # Errors
    /// None today; reserved for asset sources that can fail.
    pub fn get_asset(
        &self,
        lang: LangIdx,
        path: &str,
    ) -> Result<Option<ResourceId>, ResourceError> {
        let rel = paths::clean(&format!("/{path}"));
        let rel = rel.trim_start_matches('/');
        if rel.is_empty() {
            return Ok(None);
        }
        let Some(file) = self
            .cfg
            .vfs
            .as_ref()
            .and_then(|v| v.open(Component::Assets, rel))
        else {
            return Ok(None);
        };
        let body = Body::File(file.abs);
        let lang = self.global_lang(lang);
        self.assets
            .get_or_try((lang, rel.to_owned()), || {
                Ok(self.push_asset(lang, rel, body))
            })
            .map(Some)
    }

    fn push_asset(&self, lang: LangIdx, rel: &str, body: Body) -> ResourceId {
        let link = format!("/{rel}");
        self.push(NewResource {
            origin: Origin::Asset {
                path: rel.to_owned(),
            },
            media_type: self.media_type_of(rel),
            name: link.clone(),
            name_normalized: None,
            title: link.clone(),
            params: Params::default(),
            data: Map::new(),
            lang,
            target: self.global_target(lang, &link),
            link: UrlPath::new(&link),
            body,
            policy: PublishPolicy::OnReference,
            kind: None,
        })
    }

    fn asset_files(&self) -> Result<Arc<Vec<String>>, ResourceError> {
        let mut cached = lock(&self.asset_files);
        if let Some(files) = &*cached {
            return Ok(Arc::clone(files));
        }
        let mut files: Vec<String> = match &self.cfg.vfs {
            Some(vfs) => vfs
                .walk(Component::Assets)?
                .into_iter()
                .map(|f| f.rel)
                .collect(),
            None => Vec::new(),
        };
        files.sort();
        files.dedup();
        let files = Arc::new(files);
        *cached = Some(Arc::clone(&files));
        Ok(files)
    }

    /// `resources.Match`: the assets whose path matches the glob `pattern` (Go's globs, case
    /// folded, a leading `/` ignored), sorted by path.
    ///
    /// # Errors
    /// An invalid pattern, or an assets directory that cannot be read.
    pub fn find_assets(
        &self,
        lang: LangIdx,
        pattern: &str,
    ) -> Result<Vec<ResourceId>, ResourceError> {
        let g = glob::compile(pattern.trim_start_matches('/'), GlobOpts::default())?;
        let mut paths: Vec<String> = self
            .asset_files()?
            .iter()
            .filter(|p| g.is_match(p))
            .cloned()
            .collect();
        paths.sort();
        paths.dedup();
        let mut ids = Vec::with_capacity(paths.len());
        for p in &paths {
            if let Some(id) = self.get_asset(lang, p)? {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// `resources.GetMatch`: the first of [`find_assets`](Self::find_assets).
    ///
    /// # Errors
    /// As [`find_assets`](Self::find_assets).
    pub fn find_asset(
        &self,
        lang: LangIdx,
        pattern: &str,
    ) -> Result<Option<ResourceId>, ResourceError> {
        Ok(self.find_assets(lang, pattern)?.into_iter().next())
    }

    // ── bundles ─────────────────────────────────────────────────────────────────────────────

    /// A bundle file of a page. Registering a second file for the same target returns the
    /// first registration (translations sharing a bundle directory share the file).
    pub fn register_bundle(&self, b: &BundleResource) -> ResourceId {
        let name = b.name.trim_start_matches('/');
        let link = paths::join(&["/", &b.dir, name]);
        let target = OutputPath::new(&format!("{}{link}", self.lang_target(b.lang).target_prefix));
        let result: Result<ResourceId, std::convert::Infallible> =
            self.bundles.get_or_try(target.clone(), || {
                Ok(self.push(NewResource {
                    origin: Origin::Bundle { lang: b.lang },
                    media_type: self.media_type_of(name),
                    name: name.to_owned(),
                    name_normalized: None,
                    title: name.to_owned(),
                    params: Params::default(),
                    data: Map::new(),
                    lang: b.lang,
                    target,
                    link: UrlPath::new(&link),
                    body: Body::File(b.file.clone()),
                    policy: b.policy,
                    kind: None,
                }))
            });
        match result {
            Ok(id) => id,
            Err(never) => match never {},
        }
    }

    /// A page resource a content adapter added: like [`register_bundle`](Self::register_bundle)
    /// (target and link below the page's directory, or the passed resource's own place), with
    /// the adapter's name, title and params. Every call makes a new resource (two adapter
    /// resources may share a passed resource's place).
    pub fn register_adapter_resource(&self, r: &AdapterResource) -> ResourceId {
        let name = r.name.trim_start_matches('/');
        let (target, link) = match &r.place {
            Some((target, link)) => (target.clone(), link.clone()),
            None => {
                let link = paths::join(&["/", &r.dir, name]);
                let target =
                    OutputPath::new(&format!("{}{link}", self.lang_target(r.lang).target_prefix));
                (target, UrlPath::new(&link))
            }
        };
        let types = &self.cfg.media_types;
        let media_type = r
            .media_type
            .as_deref()
            .and_then(|t| types.by_type(t))
            .map_or_else(|| self.media_type_of(name), |id| types.get(id).clone());
        let display = r.display_name.clone().unwrap_or_else(|| name.to_owned());
        self.push(NewResource {
            origin: Origin::Bundle { lang: r.lang },
            media_type,
            title: r.title.clone().unwrap_or_else(|| display.clone()),
            name_normalized: Some(normalize_name(name)),
            name: display,
            params: r.params.clone(),
            data: Map::new(),
            lang: r.lang,
            target,
            link,
            body: r.body.clone(),
            policy: r.policy,
            kind: None,
        })
    }

    // ── named targets ───────────────────────────────────────────────────────────────────────

    /// Claims `target` for a resource made from `input` (a hash of the inputs): the first claim
    /// makes it; a claim in another language gets the first resource; a claim in the same
    /// language with other inputs is an error.
    fn claim(
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

    fn named(&self, lang: LangIdx, target: OutputPath, link: String, body: Body) -> NewResource {
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

    // ── transforms, metadata and images ─────────────────────────────────────────────────────

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

    // ── content and publishing marks ────────────────────────────────────────────────────────

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

pub(crate) fn kind_of(mt: &MediaType) -> ResourceKind {
    if mt.main == "image" && ImageFormat::from_subtype(&mt.sub).is_some() {
        ResourceKind::Image
    } else if !mt.main.is_empty() && mt.is_text() {
        ResourceKind::Text
    } else {
        ResourceKind::Other
    }
}

pub(crate) fn empty_media_type() -> MediaType {
    MediaType {
        main: String::new(),
        sub: String::new(),
        mime_suffix: String::new(),
        suffixes: Vec::new(),
        delimiter: String::new(),
    }
}

/// Types of extensions the configured table does not know (the IANA registration first, then
/// `mime_guess`).
fn well_known_media_type(ext: &str) -> Option<MediaType> {
    let t = match ext {
        "ico" => "image/vnd.microsoft.icon".to_owned(),
        _ => mime_guess::from_ext(ext).first_raw()?.to_owned(),
    };
    let mut mt = MediaType::parse(&t).ok()?;
    mt.suffixes = vec![ext.to_owned()];
    mt.delimiter = ".".to_owned();
    Some(mt)
}

/// A resource name for case-insensitive lookups: lower case, spaces as dashes.
#[must_use]
pub(crate) fn normalize_name(name: &str) -> String {
    ssg_base::text::to_lower(name).replace(' ', "-")
}

/// `ident` inserted before the extension of the last path element (`/a/b.css` + `.min` →
/// `/a/b.min.css`; a name without extension gets it at the end).
pub(crate) fn add_identifier(path: &str, ident: &str) -> String {
    let (dir, file) = paths::split(path);
    match file.rfind('.') {
        Some(i) => format!("{dir}{}{ident}{}", &file[..i], &file[i..]),
        None => format!("{dir}{file}{ident}"),
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
