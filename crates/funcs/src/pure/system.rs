//! Environment, files and logging: `log_error`, `log_warn`, `get_env`, `read_file`,
//! `file_exists`.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ssg_base::diag::Diagnostic;
use tera::{Kwargs, TeraResult, Value};

use super::value::chain;
use super::{PureEnv, Registrar};

pub(super) fn register(r: &mut Registrar<'_>, env: &Arc<PureEnv>) {
    let e = Arc::clone(env);
    r.function("log_error", move |kw, _| {
        e.diagnostics
            .push(Diagnostic::error(kw.must_get::<&str>("message")?));
        Ok(Value::from(""))
    });
    let e = Arc::clone(env);
    r.function("log_warn", move |kw, _| {
        let mut d = Diagnostic::warning(kw.must_get::<&str>("message")?);
        if let Some(id) = kw.get::<&str>("id")? {
            d = d.with_id(id);
        }
        e.diagnostics.push(d);
        Ok(Value::from(""))
    });
    let e = Arc::clone(env);
    r.function("get_env", move |kw, _| {
        let name = kw.must_get::<&str>("name")?;
        // A name the project's `.env` defines is readable; the process environment wins.
        if let Some(v) = e.env_file.get(name) {
            return Ok(Value::from(
                std::env::var(name).unwrap_or_else(|_| v.clone()),
            ));
        }
        if !e.getenv.allows(name) {
            return Err(tera::Error::message(format!(
                "get_env: `{name}` is not allowed by security.funcs.getenv (allow it there, or \
                 define it in the project's .env file)"
            )));
        }
        Ok(Value::from(std::env::var(name).unwrap_or_default()))
    });
    let e = Arc::clone(env);
    r.function("read_file", move |kw, _| {
        let path = project_path(&e, kw)?;
        std::fs::read_to_string(&path)
            .map(Value::from)
            .map_err(|err| chain(format!("read_file `{}`", path.display()), err))
    });
    let e = Arc::clone(env);
    r.function("file_exists", move |kw, _| {
        Ok(Value::from(project_path(&e, kw)?.exists()))
    });
}

/// `path=` resolved in the project directory; a path may not leave it (`..` above the root).
fn project_path(env: &PureEnv, kw: &Kwargs) -> TeraResult<PathBuf> {
    let rel = kw.must_get::<&str>("path")?;
    let root = env.project_dir.as_deref().ok_or_else(|| {
        tera::Error::message("read_file/file_exists: this build has no project directory")
    })?;
    let mut out = root.to_path_buf();
    let mut depth = 0usize;
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(part) => {
                out.push(part);
                depth += 1;
            }
            Component::ParentDir => {
                if depth == 0 {
                    return Err(tera::Error::message(format!(
                        "`{rel}` is outside the project directory"
                    )));
                }
                out.pop();
                depth -= 1;
            }
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    Ok(out)
}
