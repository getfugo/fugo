//! Rolldown's options for a build: formats, targets, minification, loaders and globals.

use super::*;

/// The minifier settings: none, all of it, or the parts `drop` and `jsx: preserve` allow.
pub(super) fn minify(o: &JsBuildOptions) -> RawMinifyOptions {
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

pub(super) fn require_globals() -> GlobalsOutputOption {
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
pub(super) fn loader_module_type(l: Loader) -> Option<ModuleType> {
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
pub(super) fn module_type(l: Loader) -> Option<ModuleType> {
    Some(match l {
        Loader::Js => ModuleType::Js,
        Loader::Jsx => ModuleType::Jsx,
        Loader::Ts => ModuleType::Ts,
        Loader::Tsx => ModuleType::Tsx,
        _ => return None,
    })
}

impl JsBuilder {
    pub(super) fn bundler_options(
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
            // Legal comments stay (`eof` moves them afterwards: `legal::move_to_end`) unless
            // `none`; minified, the annotations go too (`@__PURE__`, coverage hints: hints for a
            // later minifier, and this is the last).
            legal_comments: Some(if o.legal_comments == LegalComments::None {
                RolldownLegalComments::None
            } else {
                RolldownLegalComments::Inline
            }),
            comments: Some(CommentsOptions {
                legal: o.legal_comments != LegalComments::None,
                annotation: !o.minify,
                jsdoc: true,
            }),
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
