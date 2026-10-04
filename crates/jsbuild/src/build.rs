//! `js.Build`: bundles an asset with rolldown, resolving imports in the assets first.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use rolldown::{
    Bundler, BundlerOptions, BundlerTransformOptions, CodeSplittingMode, Either,
    GeneratedCodeOptions, GlobalsOutputOption, InjectImport, InputItem, JsxOptions,
    LegalComments as RolldownLegalComments, ModuleType, OutputExports, OutputFormat,
    Platform as RolldownPlatform, RawCompressOptions, RawMangleOptions, RawMinifyOptions,
    RawMinifyOptionsDetailed, SourceMapType, TsConfig,
};
use rolldown_common::{CommentsOptions, Output};
use rolldown_error::BuildDiagnostic;
use rolldown_plugin::Plugin as _;

use crate::executor;
use crate::legal;
use crate::lower::es5;
use crate::options::{
    DropKind, Format, JsBuildOptions, Jsx, LegalComments, Loader, Platform, SourceMap, Target,
};
use crate::plugin::{self, BuildContext, ContextParts, SitePlugin};
use crate::resolve::{AssetEntry, Assets, ComponentResolver, dir};
use crate::sourcemap;

mod bundle;
mod bundler;
mod diagnostics;

use bundle::*;
use bundler::*;
use diagnostics::*;

/// A position in a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub file: PathBuf,
    /// 1-based.
    pub line: u32,
    /// 0-based, in bytes.
    pub column: u32,
}

/// A bundler error or warning, with its position in a real file where there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub text: String,
    /// The plugin that raised it.
    pub plugin: Option<String>,
    pub position: Option<Position>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(p) = &self.position {
            write!(f, "{}:{}:{}: ", p.file.display(), p.line, p.column)?;
        }
        f.write_str(&self.text)
    }
}

/// A failed `js.Build`.
#[derive(Debug, thiserror::Error)]
pub enum JsBuildError {
    /// The asset is not JavaScript, TypeScript, JSX or TSX.
    #[error("js.Build cannot build {0:?} content")]
    UnsupportedMediaType(String),
    /// An `inject` path is absolute.
    #[error("js.Build inject {0:?}: the path must be relative to the assets directory")]
    InjectAbsolute(String),
    /// An `inject` path is not an asset.
    #[error("js.Build inject {0:?}: no such asset")]
    InjectNotFound(String),
    /// The bundler reported errors (at least one).
    #[error("{}{}", .0[0], more(.0.len()))]
    Build(Vec<Diagnostic>),
    /// The bundler failed in an unexpected way (it could not start, or it panicked).
    #[error("js.Build: {0}")]
    Internal(String),
    /// The bundler's output is not what was asked for.
    #[error("unexpected bundler output: {0}")]
    Output(String),
}

fn more(n: usize) -> String {
    match n {
        0 | 1 => String::new(),
        n => format!(" (and {} more errors)", n - 1),
    }
}

/// The script to build.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    /// The asset path (`js/main.js`; a leading `/` is ignored).
    pub path: &'a str,
    /// Its media type (`text/javascript`, `text/typescript`, `text/tsx`, `text/jsx`).
    pub media_type: &'a str,
    pub contents: &'a [u8],
}

/// A built script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsBuildOutput {
    /// Where the script is published: `targetPath`, else the source path with a `.js`
    /// extension.
    pub target_path: String,
    pub code: Vec<u8>,
    /// For `external` and `linked` source maps: published at `target_path` + `.map`.
    pub source_map: Option<Vec<u8>>,
    /// For `legalComments` `external` and `linked`, when the script has legal comments: published
    /// at `target_path` + `.LEGAL.txt`.
    pub legal: Option<Vec<u8>>,
    pub warnings: Vec<Diagnostic>,
}

/// Runs `js.Build` for one project.
#[derive(Clone, Debug)]
pub struct JsBuilder {
    working_dir: PathBuf,
    publish_dir: PathBuf,
    tsconfig: Option<PathBuf>,
}

impl JsBuilder {
    /// `working_dir` is the project directory (its `node_modules` serve imports that are not
    /// assets); `publish_dir` is where source map paths are relative to.
    #[must_use]
    pub fn new(working_dir: PathBuf, publish_dir: PathBuf) -> Self {
        Self {
            working_dir,
            publish_dir,
            tsconfig: None,
        }
    }

    /// Uses this `tsconfig.json` (or `jsconfig.json`); without one, each module uses the
    /// nearest `tsconfig.json` above it.
    #[must_use]
    pub fn with_tsconfig(mut self, tsconfig: Option<PathBuf>) -> Self {
        self.tsconfig = tsconfig;
        self
    }

    /// Bundles `source` with `options`.
    ///
    /// Blocks until the build is done (see the executor's notes): call it from our render
    /// pool or a plain thread, not from a worker of rayon's global pool.
    ///
    /// # Errors
    /// An unsupported media type, a bad `inject` path, bundler errors, or a bundler failure.
    pub fn build(
        &self,
        assets: Arc<dyn Assets>,
        source: &Source<'_>,
        options: &JsBuildOptions,
    ) -> Result<JsBuildOutput, JsBuildError> {
        let entry_type = Loader::from_media_type(source.media_type)
            .and_then(module_type)
            .ok_or_else(|| JsBuildError::UnsupportedMediaType(source.media_type.to_owned()))?;
        let source_path = source.path.trim_start_matches('/');
        // The entry is known by its asset's path, which a concatenation does not have.
        let entry_id = match assets.entry(source_path) {
            Some(AssetEntry::File(f)) => f,
            _ => self.working_dir.join(source_path),
        };
        let resolver = ComponentResolver::new(assets);

        let inject = options
            .inject
            .iter()
            .map(|p| {
                if p.starts_with('/') || Path::new(p).is_absolute() {
                    return Err(JsBuildError::InjectAbsolute(p.clone()));
                }
                resolver
                    .resolve(&p.replace('\\', "/"))
                    .ok_or_else(|| JsBuildError::InjectNotFound(p.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let params = match &options.params {
            Some(p) => serde_json::to_string(p),
            None => Ok("{}".to_owned()),
        }
        .map_err(|e| JsBuildError::Output(format!("params: {e}")))?;
        let target_path = options
            .target_path
            .clone()
            .unwrap_or_else(|| with_js_extension(source_path));
        let ctx = Arc::new(BuildContext::new(ContextParts {
            resolver,
            working_dir: self.working_dir.clone(),
            entry_id: entry_id.to_string_lossy().into_owned(),
            entry_source: String::from_utf8_lossy(source.contents).into_owned(),
            entry_type,
            source_dir: dir(source_path).to_owned(),
            params,
            shims: options.shims.clone(),
            externals: options.externals.clone(),
            inject: inject.clone(),
            loaders: options.loaders.clone(),
            es5: options.target == Target::Es5,
            legacy_decorators: self.tsconfig.as_deref().is_some_and(legacy_decorators),
            sourcemap: options.source_map != SourceMap::None,
        }));
        let bundler_options = self.bundler_options(options, &target_path, &inject)?;

        let built = {
            let ctx = Arc::clone(&ctx);
            executor::run(async move { bundle(bundler_options, &ctx).await })
                .map_err(JsBuildError::Internal)?
        }
        .map_err(JsBuildError::Build)?;

        let (mut code, mut map) = if options.target == Target::Es5 {
            lower_to_es5(&built, options)?
        } else {
            (built.code, built.map)
        };
        let mut legal_file = None;
        match options.legal_comments {
            LegalComments::Eof => (code, map) = legal::move_to_end(code, map),
            LegalComments::External | LegalComments::Linked => {
                let comments;
                (code, map, comments) = legal::extract(code, map);
                if !comments.is_empty() {
                    if options.legal_comments == LegalComments::Linked {
                        let name = target_path.rsplit('/').next().unwrap_or(&target_path);
                        if !code.is_empty() && !code.ends_with('\n') {
                            code.push('\n');
                        }
                        code.push_str(&format!(
                            "/*! For license information please see {name}.LEGAL.txt */\n"
                        ));
                    }
                    legal_file = Some(legal::file_text(&comments).into_bytes());
                }
            }
            LegalComments::Inline | LegalComments::None => {}
        }
        // rolldown writes no map when nothing in the output maps to a source (an entry that only
        // imports externals); js.Build still publishes one naming the entry.
        let map = map.or_else(|| {
            (options.source_map != SourceMap::None).then(|| {
                let contents = String::from_utf8_lossy(source.contents);
                serde_json::json!({
                    "version": 3,
                    "sources": [ctx.entry_id],
                    "sourcesContent": if options.sources_content {
                        serde_json::json!([contents])
                    } else {
                        serde_json::Value::Null
                    },
                    "mappings": "",
                    "names": [],
                })
                .to_string()
            })
        });
        let source_map = match (options.source_map, map) {
            (SourceMap::None, _) | (_, None) => None,
            (mode, Some(map)) => {
                let map = sourcemap::fix_sources(&map, &self.publish_dir)
                    .map_err(|e| JsBuildError::Output(format!("source map: {e}")))?;
                if !code.is_empty() && !code.ends_with('\n') {
                    code.push('\n');
                }
                match mode {
                    SourceMap::Inline => {
                        let data = base64::engine::general_purpose::STANDARD.encode(&map);
                        code.push_str(&format!(
                            "//# sourceMappingURL=data:application/json;base64,{data}\n"
                        ));
                        None
                    }
                    SourceMap::Linked => {
                        let name = target_path.rsplit('/').next().unwrap_or(&target_path);
                        code.push_str(&format!("//# sourceMappingURL={name}.map\n"));
                        Some(map)
                    }
                    SourceMap::External | SourceMap::None => Some(map),
                }
            }
        };
        Ok(JsBuildOutput {
            target_path,
            code: code.into_bytes(),
            source_map,
            legal: legal_file,
            warnings: built.warnings,
        })
    }
}
