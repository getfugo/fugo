//! `get_env`: a name the project's `.env` file defines is readable without
//! `security.funcs.getenv`, and the process environment wins over the file; any other name
//! needs the allowlist.

use std::collections::BTreeMap;
use std::sync::Arc;

use tera::Context;

use crate::support::{Harness, env};

#[test]
fn get_env_reads_the_env_file() {
    let mut e = (*env()).clone();
    e.env_file = Arc::new(BTreeMap::from([
        ("ENV_FILE_TEST_ONLY".to_owned(), "from-file".to_owned()),
        ("PATH".to_owned(), "from-file".to_owned()),
    ]));
    let h = Harness::with_env(&Arc::new(e));
    let ctx = Context::new();
    let render = |src: &str| h.render(src, &ctx);

    assert_eq!(
        render(r#"{{ get_env(name="ENV_FILE_TEST_ONLY") }}"#).as_deref(),
        Ok("from-file")
    );
    let path = std::env::var("PATH").expect("PATH is set");
    assert_eq!(
        render(r#"{{ get_env(name="PATH") | safe }}"#).as_deref(),
        Ok(path.as_str())
    );
    let err = render(r#"{{ get_env(name="HOME") }}"#).expect_err("not allowed");
    assert!(
        err.contains("not allowed by security.funcs.getenv") && err.contains(".env"),
        "{err}"
    );
}
