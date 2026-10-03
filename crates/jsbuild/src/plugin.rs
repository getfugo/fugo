//! The rolldown plugin behind `js.Build`: the entry script from memory, Go's assets-first
//! import resolution with shims and externals, `@params`, `inject`, CSS imports, and the
//! transforms rolldown lacks (the es5 check, TC39 decorators).
//!
//! Errors raised here keep their position: rolldown turns a hook error into a bare plugin
//! error, so the hooks record [`Diagnostic`]s in the context and the build reports those.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oxc::span::SourceType;
use rolldown_common::{ModuleType, ResolvedExternal};
use rolldown_plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookTransformArgs, HookTransformOutput, HookTransformReturn, HookUsage,
    Plugin, PluginContext, SharedLoadPluginContext, SharedTransformPluginContext,
};

use crate::build::{Diagnostic, Position};
use crate::css::{self, CssLoader, CssNames};
use crate::lower::{LowerError, decorators, es5};
use crate::options::Loader;
use crate::resolve::{ComponentResolver, dir, join};

/// The plugin's name in rolldown's diagnostics.
pub(crate) const PLUGIN_NAME: &str = "ssg-import-resolver";
/// The specifier of the entry script; it resolves to [`BuildContext::entry_id`].
pub(crate) const ENTRY: &str = "ssg:entry";
/// The specifier the entry imports the `inject` files through (see [`BuildContext::new`]).
const INJECT: &str = "ssg:inject";
/// The specifier lowered decorators import their helpers from.
const DECORATOR_HELPERS: &str = "ssg:decorators";
const PARAMS_ID: &str = "\0ssg-params";
const INJECT_ID: &str = "\0ssg-inject";
const DECORATOR_HELPERS_ID: &str = "\0ssg-decorators";

/// What the plugin knows about one build.
#[derive(Debug)]
pub(crate) struct BuildContext {
    pub(crate) resolver: ComponentResolver,
    /// The project directory: imports that are not assets resolve from here.
    pub(crate) working_dir: PathBuf,
    /// The entry's module id: the absolute path of its asset (which need not exist, as for a
    /// concatenation).
    pub(crate) entry_id: String,
    /// The entry's code as loaded: the source, after the `inject` import when there is one.
    pub(crate) entry_code: String,
    /// The length of that `inject` import, on line 1.
    pub(crate) entry_prefix: u32,
    pub(crate) entry_type: ModuleType,
    /// The entry's directory in the assets (`.` at the root).
    pub(crate) source_dir: String,
    /// The JSON text of `@params`.
    pub(crate) params: String,
    pub(crate) shims: BTreeMap<String, String>,
    pub(crate) externals: Vec<String>,
    /// The resolved `inject` files.
    pub(crate) inject: Vec<PathBuf>,
    /// The user's loaders by extension (`.svg`).
    pub(crate) loaders: BTreeMap<String, Loader>,
    /// `target: es5`: every module is checked for syntax esbuild cannot lower to ES5.
    pub(crate) es5: bool,
    /// The project's tsconfig enables `experimentalDecorators` (rolldown lowers those itself).
    pub(crate) legacy_decorators: bool,
    pub(crate) sourcemap: bool,
    css_names: Mutex<CssNames>,
    /// Errors the hooks raised, with their positions.
    errors: Mutex<Vec<Diagnostic>>,
}

/// The parts of a [`BuildContext`] that come from the caller.
pub(crate) struct ContextParts {
    pub(crate) resolver: ComponentResolver,
    pub(crate) working_dir: PathBuf,
    pub(crate) entry_id: String,
    pub(crate) entry_source: String,
    pub(crate) entry_type: ModuleType,
    pub(crate) source_dir: String,
    pub(crate) params: String,
    pub(crate) shims: BTreeMap<String, String>,
    pub(crate) externals: Vec<String>,
    pub(crate) inject: Vec<PathBuf>,
    pub(crate) loaders: BTreeMap<String, Loader>,
    pub(crate) es5: bool,
    pub(crate) legacy_decorators: bool,
    pub(crate) sourcemap: bool,
}

impl BuildContext {
    /// esbuild runs every `inject` file before the entry, even one whose exports nothing uses;
    /// rolldown's `inject` only imports a file where one of its exports replaces a global. So
    /// the entry also imports them, on line 1 to keep the entry's line numbers.
    pub(crate) fn new(p: ContextParts) -> Self {
        let prefix = if p.inject.is_empty() {
            String::new()
        } else {
            format!("import {INJECT:?};")
        };
        let entry_prefix = u32::try_from(prefix.len()).unwrap_or(u32::MAX);
        Self {
            resolver: p.resolver,
            working_dir: p.working_dir,
            entry_id: p.entry_id,
            entry_code: prefix + &p.entry_source,
            entry_prefix,
            entry_type: p.entry_type,
            source_dir: p.source_dir,
            params: p.params,
            shims: p.shims,
            externals: p.externals,
            inject: p.inject,
            loaders: p.loaders,
            es5: p.es5,
            legacy_decorators: p.legacy_decorators,
            sourcemap: p.sourcemap,
            css_names: Mutex::default(),
            errors: Mutex::default(),
        }
    }

    /// The errors the hooks raised.
    pub(crate) fn take_errors(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut *lock(&self.errors))
    }

    /// Records errors at positions of module `id` and returns the error rolldown gets.
    fn fail(&self, id: &str, errors: Vec<LowerError>) -> anyhow::Error {
        let is_entry = id == self.entry_id;
        let file = (!is_entry || Path::new(id).is_file()).then(|| PathBuf::from(id));
        let first = errors
            .first()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        lock(&self.errors).extend(errors.into_iter().map(|e| Diagnostic {
            text: e.message,
            plugin: None,
            position: file.clone().map(|file| Position {
                file,
                line: e.line,
                // The entry's line 1 starts with the `inject` import.
                column: if is_entry && e.line == 1 {
                    e.column.saturating_sub(self.entry_prefix)
                } else {
                    e.column
                },
            }),
        }));
        anyhow::anyhow!(first)
    }

    /// esbuild's external matching: the path itself, a package's subpaths (`pkg` covers
    /// `pkg/sub`), and patterns with one `*` wildcard.
    fn is_external(&self, path: &str) -> bool {
        self.externals.iter().any(|e| match e.split_once('*') {
            Some((pre, suf)) => {
                path.len() >= pre.len() + suf.len() && path.starts_with(pre) && path.ends_with(suf)
            }
            None => {
                path == e
                    || (!e.starts_with('.')
                        && !e.starts_with('/')
                        && path
                            .strip_prefix(e.as_str())
                            .is_some_and(|rest| rest.starts_with('/')))
            }
        })
    }

    /// The user's loader for a path: the longest matching extension (`.module.css` before
    /// `.css`).
    fn loader(&self, path: &str) -> Option<Loader> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        self.loaders
            .iter()
            .filter(|(ext, _)| name.len() > ext.len() && name.ends_with(ext.as_str()))
            .max_by_key(|(ext, _)| ext.len())
            .map(|(_, l)| *l)
    }

    /// The CSS loader of a path: the user's, else esbuild's defaults (`.module.css` is
    /// `local-css`, `.css` is `css`).
    pub(crate) fn css_loader(&self, path: &str) -> Option<CssLoader> {
        match self.loader(path) {
            Some(Loader::Css) => Some(CssLoader::Css),
            Some(Loader::GlobalCss) => Some(CssLoader::GlobalCss),
            Some(Loader::LocalCss) => Some(CssLoader::LocalCss),
            Some(Loader::Default) | None if path.ends_with(".module.css") => {
                Some(CssLoader::LocalCss)
            }
            Some(Loader::Default) | None if path.ends_with(".css") => Some(CssLoader::Css),
            _ => None,
        }
    }
}

/// The `ssg-import-resolver` plugin.
#[derive(Debug)]
pub(crate) struct SitePlugin {
    pub(crate) ctx: std::sync::Arc<BuildContext>,
}

impl Plugin for SitePlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed(PLUGIN_NAME)
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load | HookUsage::Transform
    }

    async fn resolve_id(
        &self,
        pctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        let ctx = &self.ctx;
        let virtual_id = match args.specifier {
            ENTRY if args.is_entry => Some(ctx.entry_id.as_str()),
            "@params" | "@params/config" => Some(PARAMS_ID),
            INJECT => Some(INJECT_ID),
            DECORATOR_HELPERS => Some(DECORATOR_HELPERS_ID),
            _ => None,
        };
        if let Some(id) = virtual_id {
            return Ok(Some(HookResolveIdOutput::from_id(id)));
        }
        let (mut imp, shimmed) = match ctx.shims.get(args.specifier) {
            Some(shim) => (shim.clone(), true),
            None => (args.specifier.to_owned(), false),
        };
        if ctx.is_external(&imp) {
            return Ok(Some(HookResolveIdOutput {
                id: imp.into(),
                external: Some(ResolvedExternal::Bool(true)),
                ..Default::default()
            }));
        }
        let importer = args.importer.unwrap_or_default();
        // esbuild resolves a bare path in CSS (`composes: a from "b.css"`) next to the CSS file
        // first.
        if ctx.css_loader(importer).is_some()
            && !imp.starts_with(['.', '/'])
            && !imp.contains(':')
            && let Ok(resolved) = pctx
                .resolve(&format!("./{imp}"), Some(importer), None)
                .await?
        {
            return Ok(Some(HookResolveIdOutput::from_resolved_id(resolved)));
        }
        let rel_dir = if importer == ctx.entry_id {
            ctx.source_dir.clone()
        } else if let Some(p) = ctx.resolver.assets().assets_path(Path::new(importer)) {
            dir(&p).to_owned()
        } else if shimmed {
            ctx.source_dir.clone()
        } else {
            // An import from outside the assets (node_modules): rolldown's job.
            return Ok(None);
        };
        if !rel_dir.is_empty() && imp.starts_with('.') {
            imp = join(&rel_dir, &imp);
        }
        if let Some(file) = ctx.resolver.resolve(&imp) {
            return Ok(Some(HookResolveIdOutput::from_id(
                file.to_string_lossy().into_owned(),
            )));
        }
        // Not an asset: the original path, from the project directory (esbuild's resolveDir
        // for the entry and for modules loaded as assets).
        let from = ctx.working_dir.join("__ssg_importer__.js");
        match pctx
            .resolve(args.specifier, Some(&from.to_string_lossy()), None)
            .await?
        {
            Ok(resolved) => Ok(Some(HookResolveIdOutput::from_resolved_id(resolved))),
            Err(_) => Ok(None),
        }
    }

    async fn load(&self, _: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let ctx = &self.ctx;
        let (code, module_type) = if args.id == ctx.entry_id {
            (ctx.entry_code.clone(), ctx.entry_type.clone())
        } else if args.id == PARAMS_ID {
            (ctx.params.clone(), ModuleType::Json)
        } else if args.id == INJECT_ID {
            let imports = ctx
                .inject
                .iter()
                .map(|f| format!("import {:?};\n", f.to_string_lossy()))
                .collect();
            (imports, ModuleType::Js)
        } else if args.id == DECORATOR_HELPERS_ID {
            (decorators::DECORATOR_HELPERS.to_owned(), ModuleType::Js)
        } else if let Some(loader) = ctx.css_loader(args.id) {
            // rolldown no longer bundles CSS, and js.Build drops the CSS esbuild wrote anyway:
            // what matters is what the script sees.
            let path = Path::new(args.id);
            let source = std::fs::read_to_string(path).map_err(|e| {
                ctx.fail(
                    args.id,
                    vec![LowerError {
                        message: format!("cannot read {}: {e}", path.display()),
                        line: 1,
                        column: 0,
                    }],
                )
            })?;
            let js = css::css_module_js(loader, &source, path, &mut lock(&ctx.css_names))
                .map_err(|e| ctx.fail(args.id, vec![e]))?;
            (js, ModuleType::Js)
        } else if is_untyped_asset(ctx, args.id) {
            // Go loads an imported asset with an unknown extension as JavaScript.
            let code = std::fs::read_to_string(args.id)?;
            (code, ModuleType::Js)
        } else {
            return Ok(None);
        };
        Ok(Some(HookLoadOutput {
            code: code.into(),
            module_type: Some(module_type),
            ..Default::default()
        }))
    }

    async fn transform(
        &self,
        _: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        let ctx = &self.ctx;
        let Some(source_type) = source_type(args.module_type) else {
            return Ok(None);
        };
        if args.id.starts_with('\0') {
            return Ok(None);
        }
        if ctx.es5 {
            let errors = es5::check_es5(args.code, source_type, args.id);
            if !errors.is_empty() {
                return Err(ctx.fail(args.id, errors));
            }
        }
        if ctx.legacy_decorators {
            return Ok(None);
        }
        let lowered = decorators::lower_decorators(
            args.code,
            source_type,
            args.id,
            DECORATOR_HELPERS,
            ctx.sourcemap,
        )
        .map_err(|errors| ctx.fail(args.id, errors))?;
        Ok(lowered.map(|l| HookTransformOutput {
            code: Some(l.code),
            map: l.map.map_or(
                rolldown_plugin::HookTransformOutputMap::Null,
                rolldown_plugin::HookTransformOutputMap::from,
            ),
            ..Default::default()
        }))
    }
}

/// An asset with an extension neither rolldown nor the user gives a loader.
fn is_untyped_asset(ctx: &BuildContext, id: &str) -> bool {
    // rolldown's own module types by extension.
    const TYPED: [&str; 11] = [
        ".js", ".mjs", ".cjs", ".jsx", ".ts", ".mts", ".cts", ".tsx", ".json", ".txt", ".css",
    ];
    ctx.loader(id).is_none()
        && !TYPED.iter().any(|ext| id.ends_with(ext))
        && ctx.resolver.assets().assets_path(Path::new(id)).is_some()
}

/// The source type the transforms parse a module with.
fn source_type(module_type: &ModuleType) -> Option<SourceType> {
    Some(match module_type {
        ModuleType::Js => SourceType::mjs(),
        ModuleType::Jsx => SourceType::jsx(),
        ModuleType::Ts => SourceType::ts(),
        ModuleType::Tsx => SourceType::tsx(),
        _ => return None,
    })
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
