//! A Tera instance with the pure functions, and rendering helpers.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use ssg_base::Clock;
use ssg_funcs::{Locales, PureEnv, register_pure};
use tera::{Context, Kwargs, State, Tera, Value};

/// The crate directory (`crates/funcs`).
pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A fixed environment: English default, Thai second, clock at 2024-07-14T10:31:59Z, UTC, the
/// crate directory as project directory.
pub fn env() -> Arc<PureEnv> {
    let mut env = PureEnv::new("en");
    env.locales = Locales::new("en", ["th"]);
    env.clock = Clock("2024-07-14T10:31:59Z".parse().expect("valid timestamp"));
    env.project_dir = Some(crate_dir());
    Arc::new(env)
}

/// A Tera instance with every pure name, plus `__capture`, which records the value (and its
/// safety) that reaches it.
pub struct Harness {
    pub tera: Tera,
    captured: Arc<Mutex<Option<Value>>>,
}

impl Harness {
    pub fn new() -> Self {
        Self::with_env(&env())
    }

    pub fn with_env(env: &Arc<PureEnv>) -> Self {
        let mut tera = Tera::default();
        register_pure(&mut tera, env);
        let captured: Arc<Mutex<Option<Value>>> = Arc::default();
        let slot = Arc::clone(&captured);
        tera.register_filter("__capture", move |v: Value, _: Kwargs, _: &State| {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(v);
            Value::from("")
        });
        Self { tera, captured }
    }

    /// Renders `src` (autoescaped) with `ctx`.
    pub fn render(&self, src: &str, ctx: &Context) -> Result<String, String> {
        self.tera
            .render_str(src, ctx, true)
            .map_err(|e| format!("{e:#}"))
    }

    /// Evaluates `expr` and returns the value (with its safety flag).
    pub fn eval(&self, expr: &str, ctx: &Context) -> Result<Value, String> {
        *self.captured.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.render(&format!("{{{{ {expr} | __capture }}}}"), ctx)?;
        self.captured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| "no value".to_owned())
    }
}
