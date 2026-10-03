//! `js.Build`: bundles an asset with rolldown, resolving imports in the assets first.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use rolldown::{
    Bundler, BundlerOptions, BundlerTransformOptions, CodeSplittingMode, Either,
    GeneratedCodeOptions, GlobalsOutputOption, InjectImport, InputItem, JsxOptions, LegalComments,
    ModuleType, OutputExports, OutputFormat, Platform as RolldownPlatform, RawCompressOptions,
    RawMangleOptions, RawMinifyOptions, RawMinifyOptionsDetailed, SourceMapType, TsConfig,
};
use rolldown_common::Output;
use rolldown_error::BuildDiagnostic;
use rolldown_plugin::Plugin as _;

use crate::executor;
use crate::lower::es5;
use crate::options::{DropKind, Format, JsBuildOptions, Jsx, Loader, Platform, SourceMap, Target};
use crate::plugin::{self, BuildContext, ContextParts, SitePlugin};
use crate::resolve::{AssetEntry, Assets, ComponentResolver, dir};
use crate::sourcemap;

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

        let (mut code, map) = if options.target == Target::Es5 {
            lower_to_es5(&built, options)?
        } else {
            (built.code, built.map)
        };
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
            warnings: built.warnings,
        })
    }

    fn bundler_options(
        &self,
        o: &JsBuildOptions,
        target_path: &str,
        inject: &[PathBuf],
    ) -> Result<BundlerOptions, JsBuildError> {
        let format = match o.format {
            Format::Iife => OutputFormat::Iife,
            Format::Esm => OutputFormat::Esm,
            Format::Cjs => OutputFormat::Cjs,
        };
        let platform = match o.platform {
            Platform::Browser => RolldownPlatform::Browser,
            Platform::Node => RolldownPlatform::Node,
            Platform::Neutral => RolldownPlatform::Neutral,
        };
        let jsx = match o.jsx {
            Jsx::Preserve => Either::Left("preserve".to_owned()),
            Jsx::Automatic => Either::Right(JsxOptions {
                runtime: Some("automatic".to_owned()),
                import_source: o.jsx_import_source.clone(),
                ..Default::default()
            }),
            // rolldown defaults to the automatic runtime; js.Build's `transform` is classic.
            Jsx::Transform => Either::Right(JsxOptions {
                runtime: Some("classic".to_owned()),
                pragma: o.jsx_factory.clone(),
                pragma_frag: o.jsx_fragment.clone(),
                ..Default::default()
            }),
        };
        // rolldown cannot target ES5: build ES2015, lowered afterwards.
        let target = match o.target {
            Target::Es5 => "es2015",
            t => t.as_str(),
        };

        // esbuild defines process.env.NODE_ENV for the browser when the site does not. rolldown
        // does too, but derives the value from its minify option, which `drop` changes.
        let mut define: Vec<(String, String)> = o
            .defines
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if o.platform == Platform::Browser
            && !["process", "process.env", "process.env.NODE_ENV"]
                .iter()
                .any(|k| o.defines.contains_key(*k))
        {
            let env = if o.minify {
                "production"
            } else {
                "development"
            };
            define.push(("process.env.NODE_ENV".to_owned(), format!("\"{env}\"")));
        }

        let mut native_inject = Vec::new();
        for file in inject {
            let from = file.to_string_lossy().into_owned();
            for name in exports_of(file)? {
                native_inject.push(InjectImport::named(name, None, from.clone()));
            }
        }

        let module_types: Vec<(String, ModuleType)> = o
            .loaders
            .iter()
            .filter_map(|(ext, l)| Some((ext.clone(), loader_module_type(*l)?)))
            .collect();

        let tsconfig = match &self.tsconfig {
            Some(p) => TsConfig::Manual(p.clone()),
            None => TsConfig::Auto(true),
        };

        Ok(BundlerOptions {
            input: Some(vec![InputItem {
                name: Some(stem(target_path).to_owned()),
                import: plugin::ENTRY.to_owned(),
            }]),
            cwd: Some(self.working_dir.clone()),
            dir: Some(self.publish_dir.to_string_lossy().into_owned()),
            entry_filenames: Some(
                target_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(target_path)
                    .to_owned()
                    .into(),
            ),
            asset_filenames: Some("[name]-[hash][extname]".to_owned().into()),
            format: Some(format),
            // esbuild's CommonJS output keeps a default export as `exports.default`; rolldown's
            // `auto` would make it `module.exports`. (IIFE output must keep `auto`: `named`
            // makes it read an `exports` variable.)
            exports: (o.format == Format::Cjs).then_some(OutputExports::Named),
            platform: Some(platform),
            // esbuild turns a require() of an external into a require() call in IIFE output;
            // rolldown would read a global named after the module.
            globals: (o.format == Format::Iife).then(require_globals),
            // js.Build always writes one script, dynamic imports included.
            code_splitting: Some(CodeSplittingMode::Bool(false)),
            legal_comments: Some(LegalComments::Inline),
            // For ES5, namespace objects without `Symbol.toStringTag`, which would throw in
            // engines without `Symbol`.
            generated_code: (o.target == Target::Es5).then(GeneratedCodeOptions::es5),
            sourcemap: (o.source_map != SourceMap::None).then_some(SourceMapType::Hidden),
            sourcemap_exclude_sources: Some(!o.sources_content),
            minify: Some(minify(o)),
            define: Some(define.into_iter().collect()),
            inject: (!native_inject.is_empty()).then_some(native_inject),
            module_types: (!module_types.is_empty()).then(|| module_types.into_iter().collect()),
            transform: Some(BundlerTransformOptions {
                jsx: Some(jsx),
                target: Some(Either::Left(target.to_owned())),
                ..Default::default()
            }),
            tsconfig: Some(tsconfig),
            ..Default::default()
        })
    }
}

/// The minifier settings: none, all of it, or the parts `drop` and `jsx: preserve` allow.
fn minify(o: &JsBuildOptions) -> RawMinifyOptions {
    // Mangled component names would be lower case, which JSX reads as HTML elements.
    let mangle = o.minify && o.jsx != Jsx::Preserve;
    match (o.minify, o.drop) {
        (false, None) => RawMinifyOptions::Bool(false),
        (true, None) if mangle => RawMinifyOptions::Bool(true),
        (minify, drop) => RawMinifyOptions::Object(RawMinifyOptionsDetailed {
            mangle: mangle.then(RawMangleOptions::default),
            mangle_properties: None,
            compress: Some(RawCompressOptions {
                drop_console: Some(drop == Some(DropKind::Console)),
                drop_debugger: Some(drop == Some(DropKind::Debugger)),
                ..Default::default()
            }),
            remove_whitespace: minify,
            ascii_only: false,
        }),
    }
}

fn require_globals() -> GlobalsOutputOption {
    GlobalsOutputOption::Fn(Arc::new(|id: &str| {
        let call = format!(
            "require({})",
            serde_json::to_string(id).unwrap_or_else(|_| format!("{id:?}"))
        );
        Box::pin(async move { Ok(call) })
    }))
}

/// The module type of a loader; `None` for the loaders the plugin handles (CSS) and for
/// `default` (rolldown's choice by extension).
fn loader_module_type(l: Loader) -> Option<ModuleType> {
    Some(match l {
        Loader::Base64 => ModuleType::Base64,
        Loader::Binary => ModuleType::Binary,
        Loader::DataUrl => ModuleType::Dataurl,
        Loader::Empty => ModuleType::Empty,
        Loader::File => ModuleType::Asset,
        Loader::Js => ModuleType::Js,
        Loader::Json => ModuleType::Json,
        Loader::Jsx => ModuleType::Jsx,
        Loader::Text => ModuleType::Text,
        Loader::Ts => ModuleType::Ts,
        Loader::Tsx => ModuleType::Tsx,
        Loader::Css | Loader::GlobalCss | Loader::LocalCss | Loader::Default => return None,
    })
}

/// The module type of the entry script.
fn module_type(l: Loader) -> Option<ModuleType> {
    Some(match l {
        Loader::Js => ModuleType::Js,
        Loader::Jsx => ModuleType::Jsx,
        Loader::Ts => ModuleType::Ts,
        Loader::Tsx => ModuleType::Tsx,
        _ => return None,
    })
}

/// What one build produced.
struct Built {
    code: String,
    /// The source map's JSON.
    map: Option<String>,
    warnings: Vec<Diagnostic>,
}

/// One rolldown build. Diagnostics are converted here, on the runtime, with the context that
/// locates them.
async fn bundle(
    options: BundlerOptions,
    ctx: &Arc<BuildContext>,
) -> Result<Built, Vec<Diagnostic>> {
    let plugin = SitePlugin::new_shared(SitePlugin {
        ctx: Arc::clone(ctx),
    });
    let mut bundler =
        Bundler::with_plugins(options, vec![plugin]).map_err(|e| errors(ctx, &e.into_vec()))?;
    let out = bundler.generate().await;
    // Closing only runs the closeBundle hooks, which this plugin has none of.
    let _ = bundler.close().await;
    let out = out.map_err(|e| errors(ctx, &e.into_vec()))?;

    // js.Build semantics: what esbuild rejects is an error, not a warning. A CSS module's
    // string export names (`"my-class"`) below ES2022 are rolldown's to lower, not an error.
    let (promoted, warnings): (Vec<_>, Vec<_>) = out
        .warnings
        .iter()
        .filter(|w| {
            let kind = w.kind().to_string();
            !(matches!(
                kind.as_str(),
                "MISSING_NAME_OPTION_FOR_IIFE_EXPORT" | "MISSING_GLOBAL_NAME"
            ) || (kind == "TOLERATED_TRANSFORM"
                && w.id().is_some_and(|id| ctx.css_loader(&id).is_some())))
        })
        .partition(|w| match w.kind().to_string().as_str() {
            "UNRESOLVED_IMPORT" => true,
            // oxc's transform warnings: of those, esbuild rejects only top-level await below
            // ES2017 (BigInt literals, TypeScript namespace and `export =` notes are warnings
            // there too, or work).
            "TOLERATED_TRANSFORM" => w
                .to_string()
                .starts_with("Top-level await is not available"),
            _ => false,
        });
    if !promoted.is_empty() {
        return Err(sorted(
            promoted.into_iter().map(|d| diagnostic(ctx, d)).collect(),
        ));
    }
    let chunk = out
        .assets
        .iter()
        .find_map(|o| match o {
            Output::Chunk(c) if c.is_entry => Some(c),
            _ => None,
        })
        .ok_or_else(|| {
            vec![Diagnostic {
                text: "the bundler wrote no script".to_owned(),
                plugin: None,
                position: None,
            }]
        })?;
    Ok(Built {
        code: chunk.code.clone(),
        map: chunk
            .map
            .as_ref()
            .map(oxc_sourcemap::SourceMap::to_json_string),
        warnings: warnings.into_iter().map(|d| diagnostic(ctx, d)).collect(),
    })
}

/// A failed build's errors: the plugin's own errors carry positions rolldown drops, so they
/// replace rolldown's reports of them.
fn errors(ctx: &BuildContext, diags: &[BuildDiagnostic]) -> Vec<Diagnostic> {
    let recorded = ctx.take_errors();
    // rolldown reports a hook error as a plugin error, or for `load` as a failed load naming
    // the plugin.
    let reports_ours = |d: &BuildDiagnostic| {
        d.plugin().as_deref() == Some(plugin::PLUGIN_NAME)
            || d.to_string()
                .contains(&format!("`{}`", plugin::PLUGIN_NAME))
    };
    let mut out: Vec<Diagnostic> = diags
        .iter()
        .filter(|d| recorded.is_empty() || !reports_ours(d))
        .map(|d| diagnostic(ctx, d))
        .collect();
    out.splice(0..0, recorded);
    let mut out = sorted(out);
    if out.is_empty() {
        out.push(Diagnostic {
            text: "the build failed".to_owned(),
            plugin: None,
            position: None,
        });
    }
    out
}

/// Errors in esbuild's order: by file and position (rolldown reports them as modules finish);
/// errors without a position keep their order, after the others.
fn sorted(mut diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diags.sort_by(|a, b| match (&a.position, &b.position) {
        (Some(a), Some(b)) => (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    diags
}

/// A rolldown diagnostic located in a real file, with esbuild's byte columns.
fn diagnostic(ctx: &BuildContext, d: &BuildDiagnostic) -> Diagnostic {
    let mut text = d.to_string();
    if d.kind().to_string() == "UNRESOLVED_IMPORT"
        && let Some(spec) = text.split('\'').nth(1)
    {
        // esbuild's wording, which sites and docs know.
        text = format!("Could not resolve {}", serde_json::Value::from(spec));
    }
    let position = d
        .to_diagnostic()
        .get_primary_location()
        .and_then(|(file, line, column, _)| {
            let path = d
                .id()
                .filter(|id| Path::new(id).is_absolute())
                .map_or_else(|| ctx.working_dir.join(&file), PathBuf::from);
            let line = u32::try_from(line).ok()?;
            let is_entry = path.to_string_lossy() == ctx.entry_id;
            let source = if is_entry {
                ctx.entry_code.clone()
            } else {
                std::fs::read_to_string(&path).ok()?
            };
            let mut column = byte_column(&source, line, column);
            if is_entry && line == 1 {
                column = column.saturating_sub(ctx.entry_prefix);
            }
            path.is_file().then_some(Position {
                file: path,
                line,
                column,
            })
        });
    Diagnostic {
        text,
        plugin: d.plugin(),
        position,
    }
}

/// The byte column of a UTF-16 column on a 1-based line.
fn byte_column(source: &str, line: u32, utf16_column: usize) -> u32 {
    let text = source
        .split('\n')
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX))
        .unwrap_or_default();
    let mut units = 0;
    let mut bytes = 0;
    for ch in text.chars() {
        if units >= utf16_column {
            break;
        }
        units += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    u32::try_from(bytes).unwrap_or(u32::MAX)
}

/// The `es5` target's last step, with the source maps of both steps collapsed.
fn lower_to_es5(
    built: &Built,
    o: &JsBuildOptions,
) -> Result<(String, Option<String>), JsBuildError> {
    let lowered = es5::lower_to_es5(&built.code, o.minify, built.map.is_some()).map_err(|e| {
        JsBuildError::Build(vec![Diagnostic {
            text: e.message,
            plugin: None,
            position: None,
        }])
    })?;
    let map = match (&built.map, lowered.map) {
        (Some(first), Some(second)) => {
            let first = oxc_sourcemap::SourceMap::from_json_string(first)
                .map_err(|e| JsBuildError::Output(format!("source map: {e}")))?;
            Some(rolldown_sourcemap::collapse_sourcemaps(&[&first, &second]).to_json_string())
        }
        _ => None,
    };
    Ok((lowered.code, map))
}

/// The names an `inject` file exports: the globals its exports replace.
fn exports_of(file: &Path) -> Result<Vec<String>, JsBuildError> {
    use oxc::allocator::Allocator;
    use oxc::ast::ast::{Declaration, Statement};
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    let source = std::fs::read_to_string(file)
        .map_err(|e| JsBuildError::Output(format!("inject {}: {e}", file.display())))?;
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file).unwrap_or_default();
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    let mut names = Vec::new();
    for stmt in &parsed.program.body {
        match stmt {
            Statement::ExportDeclaration(d) => match &d.declaration {
                Declaration::FunctionDeclaration(f) => {
                    names.extend(f.id.as_ref().map(|i| i.name.to_string()));
                }
                Declaration::ClassDeclaration(c) => {
                    names.extend(c.id.as_ref().map(|i| i.name.to_string()));
                }
                Declaration::VariableDeclaration(v) => {
                    for d in &v.declarations {
                        names.extend(
                            d.id.get_binding_identifiers()
                                .iter()
                                .map(|i| i.name.to_string()),
                        );
                    }
                }
                _ => {}
            },
            Statement::ExportNamedDeclaration(d) => {
                names.extend(d.specifiers.iter().map(|s| s.exported.name().to_string()));
            }
            _ => {}
        }
    }
    Ok(names)
}

/// Whether a tsconfig turns on `experimentalDecorators` (a text search, since tsconfig files
/// may hold comments and trailing commas).
fn legacy_decorators(tsconfig: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(tsconfig) else {
        return false;
    };
    text.match_indices("\"experimentalDecorators\"")
        .any(|(i, key)| {
            text[i + key.len()..]
                .trim_start()
                .strip_prefix(':')
                .is_some_and(|v| v.trim_start().starts_with("true"))
        })
}

fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').map_or(name, |i| &name[..i])
}

fn with_js_extension(path: &str) -> String {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    match path[name_start..].rfind('.') {
        Some(dot) => format!("{}.js", &path[..name_start + dot]),
        None => format!("{path}.js"),
    }
}
