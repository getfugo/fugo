//! The resource pipes (REWRITE_PLAN.md §2.1, §3.4, task T42): every [`Transform`] beyond
//! `fingerprint`, `post_process` placeholders and `execute_as_template`.
//!
//! | Transform | Implementation | Target |
//! |---|---|---|
//! | [`Transform::Minify`] | `ssg-minify` by media type (no minifier: an error) | `.min` before the extension |
//! | [`Transform::ToCss`] | grass (dart-sass semantics); imports through the assets view, `includePaths`, `build:vars` | `targetPath`, else `.css` |
//! | [`Transform::JsBuild`] | rolldown through `ssg-jsbuild` | `targetPath`, else `.js` |
//! | [`Transform::Fingerprint`] | the store (T40) | `.<hex digest>` before the extension |
//!
//! **Laziness.** [`ResourceStore::transform`] registers the result at once with its final
//! target, link and media type, and a [`Body::Pending`] body; the work runs when the content is
//! needed ([`ResourceStore::realize`], [`ResourceStore::content`], publishing). Only a
//! `fingerprint` depends on the content for its link: over a computed resource it is computed
//! at once; over a pending one it is pending too, its record provisional (the source's link,
//! [`PublishPolicy::Never`]) until computed. So a chain ending in
//! [`ResourceStore::post_process`] runs in build phase E5, as long as nobody asks for its
//! content or its fingerprinted link earlier (the crate README says how template functions
//! build views of pending results). Every transform runs in process: there are no external
//! tools.

mod assets;
mod jsbuild;
mod minify;
mod postprocess;
mod sass;
mod sass_imports;
mod template;

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use serde_json::Value as Json;
use ssg_base::paths::{self, OutputPath, UrlPath};
use ssg_base::{ResourceId, Value};
use ssg_config::{Config, MediaType};
use ssg_jsbuild::{JsBuildError, JsBuildOptions, JsBuilder, OptionsError};
use ssg_minify::{Minifier, MinifyError};

use crate::store::{
    Body, HashAlgo, NewResource, Origin, PublishPolicy, Resource, ResourceError, ResourceStore,
    add_identifier, kind_of, lock,
};

pub use postprocess::{PostProcessId, PpField, has_placeholder};
pub use sass::{OutputStyle, SassVar, ToCssOptions};
pub use template::TemplateExecutor;

/// A transform of one resource into another (see the module table).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Transform {
    /// `fingerprint`: the content is unchanged; the target gets `.<hex digest>` before its
    /// extension and `Data.Integrity` is `<algo>-<base64 digest>` (subresource integrity).
    Fingerprint(HashAlgo),
    /// `minify`: by the media type's minifier (HTML, CSS, JS, JSON, SVG, XML).
    Minify,
    /// `to_css` (`css.Sass`, `toCSS`).
    ToCss(ToCssOptions),
    /// `js_build` (`js.Build`); boxed, as its options are much larger than the others'.
    JsBuild(Box<JsBuildSpec>),
}

impl Transform {
    /// The template function's name (`to_css`), as errors name the transform.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Fingerprint(_) => "fingerprint",
            Self::Minify => "minify",
            Self::ToCss(_) => "to_css",
            Self::JsBuild(_) => "js_build",
        }
    }
}

/// `js_build` options with the identity a transform needs: two specs are the same transform
/// when their options are equal (`params` compared as JSON).
#[derive(Clone, Debug, PartialEq)]
pub struct JsBuildSpec(pub JsBuildOptions);

impl JsBuildSpec {
    /// Decodes the template's options map (`null`: the defaults).
    ///
    /// # Errors
    /// An option of the wrong type or with an unknown value.
    pub fn from_json(v: &Json) -> Result<Self, PipeError> {
        Ok(Self(JsBuildOptions::from_json(v)?))
    }
}

// JSON numbers are finite, so the options' equality is reflexive.
impl Eq for JsBuildSpec {}

impl Hash for JsBuildSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The options' Debug form is deterministic (sorted maps); equal options hash equally.
        format!("{:?}", self.0).hash(state);
    }
}

/// Why a transform failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PipeError {
    /// An option of a transform could not be decoded.
    #[error("option {option:?}: {reason}")]
    Option { option: String, reason: String },
    #[error(transparent)]
    JsOptions(#[from] OptionsError),
    /// A Sass compilation error at a position (`build:vars` for the variables sheet).
    #[error("{file}:{line}:{column}: {message}")]
    Sass {
        file: String,
        line: usize,
        column: usize,
        message: String,
    },
    /// Sass could not read its input.
    #[error("{0}")]
    SassInput(String),
    /// A CSS `@import` of a missing asset (`inlineImports`).
    #[error("{file}:{line}: @import of {path:?}: no such asset")]
    CssImport {
        file: String,
        line: usize,
        path: String,
    },
    #[error(transparent)]
    JsBuild(#[from] JsBuildError),
    #[error(transparent)]
    Minify(#[from] MinifyError),
    /// `minify` of a media type no minifier handles.
    #[error("no minifier for media type {0:?}")]
    NoMinifier(String),
    /// The template of `execute_as_template` failed.
    #[error("{0}")]
    Template(String),
    /// A text transform got content that is not UTF-8.
    #[error("the content is not UTF-8")]
    NotUtf8,
    #[error("{what}: {source}")]
    Io {
        what: String,
        source: std::io::Error,
    },
}

/// Everything the pipes need from the build: directories, the environment, the tools, the
/// `js_build` bundler and the minifier. One per build, shared by the store.
pub struct TransformEnv {
    /// The project directory: the base of `includePaths` and of `js_build`'s `node_modules`
    /// lookups.
    pub project_dir: PathBuf,
    /// The absolute publish directory (what `js_build` source maps are relative to).
    pub publish_dir: PathBuf,
    /// The build environment (`production`, `development`).
    pub environment: String,
    pub minifier: Arc<Minifier>,
    js_builder: OnceLock<JsBuilder>,
}

impl std::fmt::Debug for TransformEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformEnv")
            .field("project_dir", &self.project_dir)
            .field("publish_dir", &self.publish_dir)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}

impl Default for TransformEnv {
    /// The current directory as project, `public` in it, `production` and the default
    /// minifier.
    fn default() -> Self {
        let project_dir = std::env::current_dir().unwrap_or_default();
        Self::new(
            project_dir.clone(),
            project_dir.join("public"),
            "production".to_owned(),
        )
    }
}

impl TransformEnv {
    /// An environment for a project directory, a publish directory and an environment name;
    /// the other fields as in [`Default`].
    #[must_use]
    pub fn new(project_dir: PathBuf, publish_dir: PathBuf, environment: String) -> Self {
        Self {
            project_dir,
            publish_dir,
            environment,
            minifier: Arc::new(Minifier::default()),
            js_builder: OnceLock::new(),
        }
    }

    /// The environment of a loaded project: its directories, environment name and minifier
    /// configuration.
    /// An invalid `[minify]` table gives the default minifier, an invalid browserslist
    /// configuration no CSS targets (the publisher reports both).
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        let publish_dir = cfg.project_dir.join(&cfg.dirs.publish);
        let mut env = Self::new(
            cfg.project_dir.clone(),
            publish_dir,
            cfg.environment.clone(),
        );
        let browsers = ssg_minify::project_browsers(&cfg.project_dir, &cfg.environment);
        env.minifier = Arc::new(
            Minifier::new(&cfg.minify)
                .unwrap_or_default()
                .with_browsers(browsers.ok().flatten()),
        );
        env
    }

    /// The `js_build` builder, with the project's `tsconfig.json` (else `jsconfig.json`).
    fn js_builder(&self) -> &JsBuilder {
        self.js_builder.get_or_init(|| {
            let tsconfig = ["tsconfig.json", "jsconfig.json"]
                .into_iter()
                .map(|n| self.project_dir.join(n))
                .find(|p| p.is_file());
            JsBuilder::new(self.project_dir.clone(), self.publish_dir.clone())
                .with_tsconfig(tsconfig)
        })
    }
}

/// The pipes' state in a store: realization locks, companion files, post-processed resources.
#[derive(Default)]
pub(crate) struct PipeState {
    realizing: Mutex<HashMap<ResourceId, Arc<Mutex<()>>>>,
    /// Files published with a resource (source maps).
    companions: Mutex<BTreeMap<ResourceId, Vec<ResourceId>>>,
    pub(crate) post: postprocess::Registry,
}

impl PipeState {
    /// The companions of `ids`.
    pub(crate) fn companions_of(
        &self,
        ids: &std::collections::BTreeSet<ResourceId>,
    ) -> Vec<ResourceId> {
        let c = lock(&self.companions);
        ids.iter()
            .filter_map(|id| c.get(id))
            .flatten()
            .copied()
            .collect()
    }
}

/// What running a transform produced.
struct Output {
    bytes: Vec<u8>,
    /// A source map published next to the result (`<target>.map`).
    source_map: Option<Vec<u8>>,
}

/// Registers the result of `t` on `id` (see the module documentation).
pub(crate) fn start(
    store: &ResourceStore,
    id: ResourceId,
    t: Transform,
) -> Result<ResourceId, ResourceError> {
    let src = store.resource(id);
    let (link, target, media_type, policy) = match &t {
        Transform::Fingerprint(_) => (
            src.link.as_str().to_owned(),
            src.target.clone(),
            src.media_type.clone(),
            PublishPolicy::Never,
        ),
        Transform::Minify => (
            add_identifier(src.link.as_str(), ".min"),
            OutputPath::new(&add_identifier(src.target.as_str(), ".min")),
            src.media_type.clone(),
            PublishPolicy::OnReference,
        ),
        Transform::ToCss(o) => {
            let (link, target) = retarget(store, &src, o.target_path.as_deref(), ".css");
            (
                link,
                target,
                media_type(store, "text/css"),
                PublishPolicy::OnReference,
            )
        }
        Transform::JsBuild(o) => {
            let (link, target) = retarget(store, &src, o.0.target_path.as_deref(), ".js");
            (
                link,
                target,
                media_type(store, "text/javascript"),
                PublishPolicy::OnReference,
            )
        }
    };
    let fingerprint_now =
        matches!(t, Transform::Fingerprint(_)) && !matches!(src.body, Body::Pending);
    let new = store.push(NewResource {
        origin: Origin::Transformed {
            from: id,
            transform: Box::new(t),
        },
        kind: Some(if media_type == src.media_type {
            src.kind
        } else {
            kind_of(&media_type)
        }),
        media_type,
        name: src.name.clone(),
        name_normalized: Some(src.name_normalized.clone()),
        title: src.title.clone(),
        params: src.params.clone(),
        data: src.data.clone(),
        lang: src.lang,
        target,
        link: UrlPath::new(&link),
        body: Body::Pending,
        policy,
    });
    if fingerprint_now {
        realize(store, new)?;
    }
    Ok(new)
}

/// The link and target of a transform result: `target_path` (relative to the site root), else
/// the source's with the extension replaced by `ext`.
fn retarget(
    store: &ResourceStore,
    src: &Resource,
    target_path: Option<&str>,
    ext: &str,
) -> (String, OutputPath) {
    if let Some(tp) = target_path.filter(|t| !t.trim_start_matches('/').is_empty()) {
        let link = paths::clean(&format!("/{}", tp.trim_start_matches('/')));
        let target = store.global_target(src.lang, &link);
        return (link, target);
    }
    (
        replace_extension(src.link.as_str(), ext),
        OutputPath::new(&replace_extension(src.target.as_str(), ext)),
    )
}

/// `path` with the extension of its last element replaced by `ext` (`.css`), or `ext` appended.
fn replace_extension(path: &str, ext: &str) -> String {
    let (dir, file) = paths::split(path);
    let stem = file.rfind('.').map_or(file, |i| &file[..i]);
    format!("{dir}{stem}{ext}")
}

/// The configured media type `t` (`text/css`), else parsed.
fn media_type(store: &ResourceStore, t: &str) -> MediaType {
    let types = &store.cfg.media_types;
    types.by_type(t).map_or_else(
        || MediaType::parse(t).unwrap_or_else(|_| crate::store::empty_media_type()),
        |id| types.get(id).clone(),
    )
}

/// Computes pending resource `id` (and its sources), replacing its record.
pub(crate) fn realize(
    store: &ResourceStore,
    id: ResourceId,
) -> Result<Arc<Resource>, ResourceError> {
    let r = store.resource(id);
    if !matches!(r.body, Body::Pending) {
        return Ok(r);
    }
    let cell = Arc::clone(lock(&store.pipes.realizing).entry(id).or_default());
    let _guard = lock(&cell);
    let r = store.resource(id);
    if !matches!(r.body, Body::Pending) {
        return Ok(r);
    }
    let Origin::Transformed { from, transform } = &r.origin else {
        unreachable!("only transform results are pending");
    };
    let src = realize(store, *from)?;
    let input = store.content(*from)?;
    let env = Arc::clone(&store.cfg.transforms);
    let fail = |e: PipeError| ResourceError::Pipe {
        resource: src.name.clone(),
        transform: transform.name(),
        source: Box::new(e),
    };
    let mut done = Resource::clone(&r);
    done.policy = PublishPolicy::OnReference;
    let out = match &**transform {
        Transform::Fingerprint(algo) => {
            let digest = algo.digest(&input);
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            let integrity = format!(
                "{}-{}",
                algo.name(),
                base64::engine::general_purpose::STANDARD.encode(&digest)
            );
            done.data.insert("Integrity", Value::from(integrity));
            let ident = format!(".{hex}");
            let link = UrlPath::new(&add_identifier(src.link.as_str(), &ident));
            done.target = OutputPath::new(&add_identifier(src.target.as_str(), &ident));
            done.rel_permalink = store.rel_permalink(done.lang, &link);
            done.permalink = store.permalink(done.lang, &link);
            done.link = link;
            done.body = src.body.clone();
            store.replace(id, done);
            return Ok(store.resource(id));
        }
        Transform::Minify => minify::run(&env, &r, &input).map_err(fail)?,
        Transform::ToCss(o) => sass::run(store, &src, o, &input).map_err(fail)?,
        Transform::JsBuild(o) => jsbuild::run(store, &env, &src, &o.0, &input).map_err(fail)?,
    };
    if let Some(map) = out.source_map {
        let map_link = format!("{}.map", r.link.as_str());
        let companion = store.push(NewResource {
            origin: Origin::Named,
            media_type: media_type(store, "application/json"),
            name: map_link.clone(),
            name_normalized: None,
            title: map_link.clone(),
            params: ssg_base::Params::default(),
            data: ssg_base::Map::new(),
            lang: r.lang,
            target: OutputPath::new(&format!("{}.map", r.target.as_str())),
            link: UrlPath::new(&map_link),
            body: Body::Bytes(map.into()),
            policy: PublishPolicy::OnReference,
            kind: None,
        });
        lock(&store.pipes.companions)
            .entry(id)
            .or_default()
            .push(companion);
    }
    done.body = Body::Bytes(out.bytes.into());
    store.replace(id, done);
    Ok(store.resource(id))
}

/// `bytes` as text, for the text transforms.
fn text(bytes: &[u8]) -> Result<&str, PipeError> {
    std::str::from_utf8(bytes).map_err(|_| PipeError::NotUtf8)
}

// ── option decoding ─────────────────────────────────────────────────────────────────────────

/// The entries of an options map, keys lower-cased (options are matched case-insensitively);
/// `null` is no options.
fn option_entries(v: &Json) -> Result<Vec<(String, &Json)>, PipeError> {
    match v {
        Json::Null => Ok(Vec::new()),
        Json::Object(m) => Ok(m
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect()),
        other => Err(PipeError::Option {
            option: String::new(),
            reason: format!("the options must be a map, got {other}"),
        }),
    }
}

fn bad(option: &str, reason: impl Into<String>) -> PipeError {
    PipeError::Option {
        option: option.to_owned(),
        reason: reason.into(),
    }
}

/// A boolean option; `"true"`/`"false"` and numbers are accepted, as templates pass them.
fn opt_bool(option: &str, v: &Json) -> Result<bool, PipeError> {
    match v {
        Json::Bool(b) => Ok(*b),
        Json::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" | "" => Ok(false),
            _ => Err(bad(option, format!("expected a boolean, got {s:?}"))),
        },
        Json::Number(n) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        other => Err(bad(option, format!("expected a boolean, got {other}"))),
    }
}

/// A string option; numbers and booleans are accepted as their text.
fn opt_string(option: &str, v: &Json) -> Result<String, PipeError> {
    match v {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        Json::Bool(b) => Ok(b.to_string()),
        other => Err(bad(option, format!("expected a string, got {other}"))),
    }
}

/// A list of strings; a single string is a one-item list.
fn opt_strings(option: &str, v: &Json) -> Result<Vec<String>, PipeError> {
    match v {
        Json::Array(a) => a.iter().map(|x| opt_string(option, x)).collect(),
        other => Ok(vec![opt_string(option, other)?]),
    }
}
