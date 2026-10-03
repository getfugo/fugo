//! KaTeX in QuickJS (feature `math`): the renderer behind `to_math`.
//!
//! Go's `transform.ToMath` runs KaTeX 0.16.22 with its mhchem extension in QuickJS
//! (`internal/warpc`: the bundle `js/renderkatex.bundle.js` compiled by Javy, run on wazero by
//! a pool of instances, one JSON message per formula). This port runs the same KaTeX release (the
//! npm package's `dist/katex.min.js` and `dist/contrib/mhchem.min.js`, `assets/katex/`) in
//! QuickJS-ng through rquickjs, natively, with the same messages: `assets/katex/render.js` is
//! the entry point, rewritten from the Go implementation's `renderkatex.js` and `common.js`
//! (this repository at commit `44529028`). The output is KaTeX's, byte for byte
//! (`tests/it/math.rs` compares it with Go's on the formulas of a fixture).
//!
//! An engine (a QuickJS runtime with KaTeX loaded, about 25 ms to create) serves one formula at
//! a time. Idle engines wait in a process-wide pool: a render takes one or creates one, and puts
//! it back, so there are as many engines as formulas were ever rendered at once (Go: 8
//! instances). KaTeX keeps no state between renders but its caches, so which engine renders a
//! formula does not change the output.

use std::sync::{Mutex, PoisonError};

use rquickjs::{CatchResultExt, Context, Function, Runtime};

/// KaTeX 0.16.22 (`dist/katex.min.js` of the npm package).
const KATEX: &str = include_str!("../../assets/katex/katex.min.js");
/// KaTeX's mhchem extension (`\ce`, `\pu`), the same release (`dist/contrib/mhchem.min.js`).
const MHCHEM: &str = include_str!("../../assets/katex/mhchem.min.js");
/// The entry point: `renderKatex(json)`.
const RENDER: &str = include_str!("../../assets/katex/render.js");

/// The memory limit of one engine. Loaded, KaTeX uses about 2 MiB; the limit only stops a
/// runaway formula (Go's instances have 32 MiB of WebAssembly memory).
const MEMORY_LIMIT: usize = 128 << 20;

/// The idle engines.
static IDLE: Mutex<Vec<Engine>> = Mutex::new(Vec::new());

/// What KaTeX made of a formula: its markup and the warnings of `strict: "warn"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Rendered {
    pub output: String,
    pub warnings: Vec<String>,
}

/// Renders the JSON message `input` (`{"expression": …, "options": {…}}`, Go's
/// `warpc.KatexInput`) with `katex.renderToString`.
///
/// # Errors
/// `to_math: ` and KaTeX's error message (a parse error with `throwOnError`, or
/// `\errmessage`), or a failure of the JavaScript engine.
pub(super) fn render(input: &str) -> Result<Rendered, String> {
    let idle = IDLE.lock().unwrap_or_else(PoisonError::into_inner).pop();
    let engine = match idle {
        Some(engine) => engine,
        None => Engine::new()?,
    };
    // An engine that failed (not KaTeX: KaTeX's errors are answers) is dropped.
    let response = engine.call(input)?;
    IDLE.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(engine);
    response
}

/// A QuickJS context (which owns its runtime) with KaTeX, mhchem and the entry point loaded.
struct Engine(Context);

impl Engine {
    fn new() -> Result<Self, String> {
        let failed = |e: &dyn std::fmt::Display| format!("to_math: starting KaTeX failed: {e}");
        let runtime = Runtime::new().map_err(|e| failed(&e))?;
        runtime.set_memory_limit(MEMORY_LIMIT);
        let context = Context::full(&runtime).map_err(|e| failed(&e))?;
        context.with(|ctx| {
            for (name, source) in [("katex", KATEX), ("mhchem", MHCHEM), ("render", RENDER)] {
                ctx.eval::<(), _>(source)
                    .catch(&ctx)
                    .map_err(|e| failed(&format!("{name}: {e}")))?;
            }
            Ok::<_, String>(())
        })?;
        Ok(Self(context))
    }

    /// The response to `input`: `Ok` with KaTeX's answer (output or error), `Err` when the
    /// engine itself failed.
    fn call(&self, input: &str) -> Result<Result<Rendered, String>, String> {
        let json = self.0.with(|ctx| {
            let render: Function = ctx
                .globals()
                .get("renderKatex")
                .catch(&ctx)
                .map_err(|e| e.to_string())?;
            render
                .call::<_, String>((input,))
                .catch(&ctx)
                .map_err(|e| e.to_string())
        });
        let json = json.map_err(|e| format!("to_math: KaTeX failed: {e}"))?;
        Ok(response(&json))
    }
}

/// Decodes a response of `renderKatex` as Go's dispatcher does: a non-empty `err` is the
/// error; otherwise `output` (empty when missing) and the warnings.
fn response(json: &str) -> Result<Rendered, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("to_math: decoding the KaTeX response failed: {e}"))?;
    let string = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    let err = value.get("err").map(string).unwrap_or_default();
    if !err.is_empty() {
        return Err(format!("to_math: {err}"));
    }
    Ok(Rendered {
        output: value.get("output").map(string).unwrap_or_default(),
        warnings: value
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .map(|w| w.iter().map(string).collect())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_reports_errors() {
        let input = r##"{"expression":"x^2","options":{"output":"mathml","displayMode":false,"leqno":false,"fleqn":false,"errorColor":"#cc0000","minRuleThickness":0.04,"throwOnError":true,"strict":"error"}}"##;
        let r = render(input).unwrap();
        assert!(
            r.output.starts_with(
                "<span class=\"katex\"><math xmlns=\"http://www.w3.org/1998/Math/MathML\">"
            ),
            "{}",
            r.output
        );
        assert!(r.warnings.is_empty());
        let e = render(&input.replace("x^2", "\\\\frac{")).unwrap_err();
        assert!(e.starts_with("to_math: KaTeX parse error: "), "{e}");
    }

    #[test]
    fn engines_are_reused_across_threads() {
        let input = r#"{"expression":"\\ce{H2O}","options":{"output":"html","throwOnError":true,"strict":"error"}}"#;
        let first = render(input).unwrap();
        let outputs: Vec<Rendered> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4).map(|_| s.spawn(|| render(input).unwrap())).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(outputs.iter().all(|r| *r == first));
        assert!(!IDLE.lock().unwrap().is_empty());
    }

    #[test]
    fn response_decoding() {
        assert_eq!(
            response(r#"{"err":"boom"}"#),
            Err("to_math: boom".to_owned())
        );
        assert_eq!(
            response("{}"),
            Ok(Rendered {
                output: String::new(),
                warnings: Vec::new()
            })
        );
        assert_eq!(
            response(r#"{"output":"<b>","warnings":["w"]}"#)
                .unwrap()
                .warnings,
            ["w"]
        );
    }
}
