//! `execute_as_template` on Tera assets (a `ts/search.ts`, written with Tera
//! syntax as `sites/<site>/assets` hold them): the seam executed by a Tera executor, the
//! output a named target (same data: the same resource; other data in the same language: a
//! conflict naming both calls; in a later language: the first language's), then `js_build`.

use serde_json::{Value as J, json};
use ssg_base::diag::Position;
use ssg_base::{Idx as _, LangIdx};
use ssg_resources::pipes::JsBuildSpec;
use ssg_resources::{CallSite, PipeError, ResourceError, TemplateExecutor, Transform};

use super::{mini_site, project};

/// Tera with a call's data as the context, as the render layer runs asset templates.
struct Tera(J);

impl TemplateExecutor for Tera {
    fn execute(&self, name: &str, source: &str) -> Result<String, String> {
        let ctx = tera::Context::from_serialize(&self.0).map_err(|e| e.to_string())?;
        tera::Tera::one_off(source, &ctx, false).map_err(|e| format!("{name}: {e}"))
    }
}

const SEARCH_TS: &str = r#"interface SearchItem { title: string; url: string }
const api: string = "{{ api }}";
export function search(items: SearchItem[], q: string): SearchItem[] {
  return items.filter((i) => i.title.toLowerCase().includes(q.toLowerCase()));
}
export const endpoint = `${api}/search`;
"#;

fn call(lang: usize, line: u32) -> CallSite {
    CallSite {
        lang: LangIdx::from_index(lang),
        position: Some(Position {
            file: std::path::Path::new("layouts/_partials/footer.html").into(),
            line,
            col: 1,
        }),
    }
}

#[test]
fn execute_as_template_with_tera() {
    let site = mini_site(&[
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\ndefaultContentLanguage = \"en\"\n[languages.en]\nweight = 1\n[languages.th]\nweight = 2\n",
        ),
        ("assets/ts/search.ts", SEARCH_TS),
        ("assets/ts/bad.ts", "{{ nope( }}"),
    ]);
    let p = project(site.path(), |_| {});
    let s = &p.store;
    let src = p.asset("ts/search.ts");
    let data = Tera(json!({"api": "https://api.example.com"}));
    let id = s
        .execute_as_template(src, "ts/search.ts", &data, &call(0, 3))
        .unwrap();
    let out = String::from_utf8(s.content(id).unwrap().to_vec()).unwrap();
    assert!(
        out.contains(r#"const api: string = "https://api.example.com";"#),
        "{out}"
    );
    let r = s.resource(id);
    assert_eq!(r.media_type_string(), "text/typescript");
    assert_eq!(r.rel_permalink, "/ts/search.ts");
    // Same output: the same resource; a later language gets the first language's.
    assert_eq!(
        s.execute_as_template(src, "ts/search.ts", &data, &call(0, 9))
            .unwrap(),
        id
    );
    let other = Tera(json!({"api": "https://other.example"}));
    assert_eq!(
        s.execute_as_template(src, "ts/search.ts", &other, &call(1, 3))
            .unwrap(),
        id
    );
    // Other data in the same language: a conflict naming both call sites.
    let err = s
        .execute_as_template(src, "ts/search.ts", &other, &call(0, 12))
        .unwrap_err();
    let ResourceError::TargetConflict { first, second, .. } = &err else {
        panic!("{err}");
    };
    assert_eq!(first.position.as_ref().unwrap().line, 3);
    assert_eq!(second.position.as_ref().unwrap().line, 12);
    // A failing template names the asset.
    let err = s
        .execute_as_template(p.asset("ts/bad.ts"), "ts/bad.ts", &data, &call(0, 4))
        .unwrap_err();
    assert!(
        matches!(&err, ResourceError::Pipe { transform: "execute_as_template", source, .. }
            if matches!(**source, PipeError::Template(ref m) if m.starts_with("ts/bad.ts"))),
        "{err}"
    );
    // The executed script builds.
    let js = s
        .transform(
            id,
            Transform::JsBuild(Box::new(
                JsBuildSpec::from_json(&json!({"target": "es2015"})).unwrap(),
            )),
        )
        .unwrap();
    let code = String::from_utf8(s.content(js).unwrap().to_vec()).unwrap();
    assert!(code.contains("https://api.example.com"), "{code}");
    assert_eq!(s.resource(js).rel_permalink, "/ts/search.js");
}
