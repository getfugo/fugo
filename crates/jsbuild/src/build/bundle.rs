//! Running Rolldown, and what a build makes of its output: the ES5 pass, exports and file names.

use super::*;

/// What one build produced.
pub(super) struct Built {
    pub(super) code: String,
    /// The source map's JSON.
    pub(super) map: Option<String>,
    pub(super) warnings: Vec<Diagnostic>,
}

/// One rolldown build. Diagnostics are converted here, on the runtime, with the context that
/// locates them.
pub(super) async fn bundle(
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

/// The `es5` target's last step, with the source maps of both steps collapsed.
pub(super) fn lower_to_es5(
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
pub(super) fn exports_of(file: &Path) -> Result<Vec<String>, JsBuildError> {
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
pub(super) fn legacy_decorators(tsconfig: &Path) -> bool {
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

pub(super) fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').map_or(name, |i| &name[..i])
}

pub(super) fn with_js_extension(path: &str) -> String {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    match path[name_start..].rfind('.') {
        Some(dot) => format!("{}.js", &path[..name_start + dot]),
        None => format!("{path}.js"),
    }
}
