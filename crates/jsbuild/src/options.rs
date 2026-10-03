//! The `js.Build` options: typed, decoded once from the template's options map.
//!
//! Keys are matched case-insensitively (`targetPath`, `TargetPath` and `targetpath` are the same
//! option) and scalar values are converted leniently where sites rely on it: `minify: "true"`
//! is a boolean, `defines: {n: 42}` defines `n` as `42`, and a single string is a one-item list.
//! Enumerated values (`target`, `format`, `platform`, `sourceMap`, `jsx`, `drop`, loaders) are
//! matched case-insensitively too. Unknown keys are ignored.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value as Json;

/// A `js.Build` option that cannot be decoded.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OptionsError {
    /// The options are not a map.
    #[error("js.Build options must be a map, got {0}")]
    NotAMap(&'static str),
    /// An option has the wrong type.
    #[error("js.Build option {option:?}: expected {expected}, got {got}")]
    Type {
        option: &'static str,
        expected: &'static str,
        got: String,
    },
    /// An enumerated option has an unknown value.
    #[error("js.Build option {option:?}: unsupported value {value:?}")]
    Value { option: &'static str, value: String },
    /// The `none` loader, which the bundler has no equivalent of.
    #[error("js.Build loader \"none\" (for {extension:?}) is not supported")]
    NoneLoader { extension: String },
    /// A define key containing `=`.
    #[error("js.Build define {0:?}: a key cannot contain '='")]
    DefineKey(String),
    /// A loader extension containing `=`.
    #[error("js.Build loader extension {0:?} cannot contain '='")]
    LoaderExtension(String),
}

/// Declares a `Copy` enum of option values with its canonical spelling and accepted spellings
/// (matched case-insensitively).
macro_rules! option_enum {
    ($(#[$m:meta])* $name:ident, $option:literal {
        $($(#[$vm:meta])* $variant:ident => $cli:literal $(| $alias:literal)*),+ $(,)?
    }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($(#[$vm])* $variant),+ }

        impl $name {
            /// The canonical spelling (esbuild's).
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $cli),+ }
            }

            fn parse(s: &str) -> Result<Self, OptionsError> {
                let lower = s.to_ascii_lowercase();
                match lower.as_str() {
                    $($cli $(| $alias)* => Ok(Self::$variant),)+
                    _ => Err(OptionsError::Value { option: $option, value: s.to_owned() }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

option_enum!(
    /// The language target (`target`); the default is `esnext`.
    Target, "target" {
        Es5 => "es5",
        Es2015 => "es2015" | "es6",
        Es2016 => "es2016",
        Es2017 => "es2017",
        Es2018 => "es2018",
        Es2019 => "es2019",
        Es2020 => "es2020",
        Es2021 => "es2021",
        Es2022 => "es2022",
        Es2023 => "es2023",
        Es2024 => "es2024",
        EsNext => "esnext" | "",
    }
);

option_enum!(
    /// The output format (`format`); the default is `iife`.
    Format, "format" {
        Iife => "iife" | "",
        Esm => "esm",
        Cjs => "cjs",
    }
);

option_enum!(
    /// The platform (`platform`); the default is `browser`.
    Platform, "platform" {
        Browser => "browser" | "",
        Node => "node",
        Neutral => "neutral",
    }
);

option_enum!(
    /// How a source map is written (`sourceMap`); the default is none.
    SourceMap, "sourceMap" {
        None => "none" | "",
        /// A data URL at the end of the script.
        Inline => "inline",
        /// A separate `.map` file, not referenced from the script.
        External => "external",
        /// A separate `.map` file referenced by a `sourceMappingURL` comment.
        Linked => "linked",
    }
);

option_enum!(
    /// How JSX is handled (`jsx`); the default is `transform`.
    Jsx, "jsx" {
        Transform => "transform" | "",
        Preserve => "preserve",
        Automatic => "automatic",
    }
);

option_enum!(
    /// What to drop from the output (`drop`).
    DropKind, "drop" {
        Console => "console",
        Debugger => "debugger",
    }
);

option_enum!(
    /// A loader (`loaders` values, and the loader of the entry script), as esbuild names them.
    Loader, "loaders" {
        Base64 => "base64",
        Binary => "binary",
        Css => "css",
        DataUrl => "dataurl",
        Default => "default",
        Empty => "empty",
        /// Also accepted as `copy`.
        File => "file" | "copy",
        GlobalCss => "global-css",
        Js => "js",
        Json => "json",
        Jsx => "jsx",
        LocalCss => "local-css",
        Text => "text",
        Ts => "ts",
        Tsx => "tsx",
    }
);

impl Loader {
    /// The loader of the entry script from its media type (`text/javascript`, `text/typescript`,
    /// `text/tsx`, `text/jsx`; `application/javascript` is accepted too).
    #[must_use]
    pub fn from_media_type(media_type: &str) -> Option<Self> {
        let sub = media_type.split(';').next()?.trim().rsplit('/').next()?;
        match sub {
            "javascript" => Some(Self::Js),
            "typescript" => Some(Self::Ts),
            "tsx" => Some(Self::Tsx),
            "jsx" => Some(Self::Jsx),
            _ => None,
        }
    }

    /// The loader the Go implementation uses for an imported file with this extension (`.js`), if
    /// any.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext {
            ".js" | ".mjs" | ".cjs" => Self::Js,
            ".jsx" => Self::Jsx,
            ".ts" => Self::Ts,
            ".tsx" => Self::Tsx,
            ".css" => Self::Css,
            ".json" => Self::Json,
            ".txt" => Self::Text,
            _ => return None,
        })
    }
}

/// The options of one `js.Build` call.
#[derive(Clone, Debug, PartialEq)]
pub struct JsBuildOptions {
    /// The published path of the result (`targetPath`), slash-separated without a leading `/`.
    pub target_path: Option<String>,
    /// Minify whitespace, identifiers and syntax.
    pub minify: bool,
    pub source_map: SourceMap,
    /// Include the sources in the source map (default true).
    pub sources_content: bool,
    pub target: Target,
    pub format: Format,
    pub platform: Platform,
    /// Import paths left to the runtime.
    pub externals: Vec<String>,
    /// Asset paths (relative to the assets root) injected into every module.
    pub inject: Vec<String>,
    /// Replacements: identifier → JavaScript expression text.
    pub defines: BTreeMap<String, String>,
    pub drop: Option<DropKind>,
    /// Import path → asset path it is replaced with.
    pub shims: BTreeMap<String, String>,
    /// File extension (`.svg`) → loader.
    pub loaders: BTreeMap<String, Loader>,
    /// What `import * as params from '@params'` yields (`None`: an empty object).
    pub params: Option<Json>,
    pub jsx: Jsx,
    pub jsx_factory: Option<String>,
    pub jsx_fragment: Option<String>,
    pub jsx_import_source: Option<String>,
}

impl Default for JsBuildOptions {
    fn default() -> Self {
        Self {
            target_path: None,
            minify: false,
            source_map: SourceMap::None,
            sources_content: true,
            target: Target::EsNext,
            format: Format::Iife,
            platform: Platform::Browser,
            externals: Vec::new(),
            inject: Vec::new(),
            defines: BTreeMap::new(),
            drop: None,
            shims: BTreeMap::new(),
            loaders: BTreeMap::new(),
            params: None,
            jsx: Jsx::Transform,
            jsx_factory: None,
            jsx_fragment: None,
            jsx_import_source: None,
        }
    }
}

impl JsBuildOptions {
    /// Decodes the template's options map; `null` gives the defaults.
    ///
    /// # Errors
    /// A value of the wrong type, an unknown enumerated value, the `none` loader, or a define
    /// key or loader extension containing `=`.
    pub fn from_json(value: &Json) -> Result<Self, OptionsError> {
        let map = match value {
            Json::Null => return Ok(Self::default()),
            Json::Object(m) => m,
            other => return Err(OptionsError::NotAMap(json_kind(other))),
        };
        let mut o = Self::default();
        for (key, v) in map {
            if v.is_null() {
                continue;
            }
            match key.to_ascii_lowercase().as_str() {
                "targetpath" => {
                    let p = string("targetPath", v)?;
                    let p = p.replace('\\', "/");
                    let p = p.trim_start_matches('/');
                    o.target_path = (!p.is_empty()).then(|| p.to_owned());
                }
                "minify" => o.minify = boolean("minify", v)?,
                "sourcemap" => o.source_map = SourceMap::parse(&string("sourceMap", v)?)?,
                "sourcescontent" => o.sources_content = boolean("sourcesContent", v)?,
                "target" => o.target = Target::parse(&string("target", v)?)?,
                "format" => o.format = Format::parse(&string("format", v)?)?,
                "platform" => o.platform = Platform::parse(&string("platform", v)?)?,
                "externals" => o.externals = strings("externals", v)?,
                "inject" => o.inject = strings("inject", v)?,
                "defines" => {
                    o.defines = string_map("defines", v)?;
                    if let Some(k) = o.defines.keys().find(|k| k.contains('=')) {
                        return Err(OptionsError::DefineKey(k.clone()));
                    }
                }
                "drop" => {
                    let s = string("drop", v)?;
                    o.drop = if s.is_empty() {
                        None
                    } else {
                        Some(DropKind::parse(&s)?)
                    };
                }
                "shims" => o.shims = string_map("shims", v)?,
                "loaders" => o.loaders = loaders(v)?,
                "params" => o.params = Some(v.clone()),
                "jsx" => o.jsx = Jsx::parse(&string("jsx", v)?)?,
                "jsxfactory" => o.jsx_factory = non_empty(string("JSXFactory", v)?),
                "jsxfragment" => o.jsx_fragment = non_empty(string("JSXFragment", v)?),
                "jsximportsource" => {
                    o.jsx_import_source = non_empty(string("JSXImportSource", v)?);
                }
                // `avoidTDZ` is accepted and ignored, like Go does.
                _ => {}
            }
        }
        Ok(o)
    }

    /// The loader of an imported file: the user's loader for its extension, else Go's
    /// extension map, else JavaScript.
    #[must_use]
    pub fn loader_for(&self, filename: &str) -> Loader {
        let ext = extension(filename);
        self.loaders
            .get(ext)
            .copied()
            .or_else(|| Loader::from_extension(ext))
            .unwrap_or(Loader::Js)
    }
}

/// The extension of the last path segment, with its dot (`""` when there is none).
fn extension(filename: &str) -> &str {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    base.rfind('.').map_or("", |i| &base[i..])
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

fn json_kind(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "a list",
        Json::Object(_) => "a map",
    }
}

fn type_error(option: &'static str, expected: &'static str, got: &Json) -> OptionsError {
    OptionsError::Type {
        option,
        expected,
        got: json_kind(got).to_owned(),
    }
}

fn boolean(option: &'static str, v: &Json) -> Result<bool, OptionsError> {
    match v {
        Json::Bool(b) => Ok(*b),
        Json::Number(n) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        Json::String(s) => match s.as_str() {
            "" | "0" | "f" | "F" | "false" | "FALSE" | "False" => Ok(false),
            "1" | "t" | "T" | "true" | "TRUE" | "True" => Ok(true),
            _ => Err(OptionsError::Value {
                option,
                value: s.clone(),
            }),
        },
        other => Err(type_error(option, "a boolean", other)),
    }
}

/// A scalar as text (numbers and booleans as JavaScript writes them).
fn scalar(v: &Json) -> Option<String> {
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        Json::Bool(b) => Some(b.to_string()),
        Json::Null => Some(String::new()),
        Json::Array(_) | Json::Object(_) => None,
    }
}

fn string(option: &'static str, v: &Json) -> Result<String, OptionsError> {
    scalar(v).ok_or_else(|| type_error(option, "a string", v))
}

fn strings(option: &'static str, v: &Json) -> Result<Vec<String>, OptionsError> {
    match v {
        Json::Array(items) => items.iter().map(|i| string(option, i)).collect(),
        Json::String(s) => Ok(vec![s.clone()]),
        other => Err(type_error(option, "a list of strings", other)),
    }
}

fn string_map(option: &'static str, v: &Json) -> Result<BTreeMap<String, String>, OptionsError> {
    match v {
        Json::Object(m) => m
            .iter()
            .map(|(k, x)| Ok((k.clone(), string(option, x)?)))
            .collect(),
        other => Err(type_error(option, "a map", other)),
    }
}

fn loaders(v: &Json) -> Result<BTreeMap<String, Loader>, OptionsError> {
    string_map("loaders", v)?
        .into_iter()
        .map(|(ext, name)| {
            if ext.contains('=') {
                return Err(OptionsError::LoaderExtension(ext));
            }
            if name.eq_ignore_ascii_case("none") {
                return Err(OptionsError::NoneLoader { extension: ext });
            }
            Ok((ext, Loader::parse(&name)?))
        })
        .collect()
}
