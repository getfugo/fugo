//! The pages' side of a structure check: every page's values against the oracle's.

use super::*;

/// Checks the pages of one site.
#[allow(clippy::too_many_lines)]
pub(super) fn check_pages(
    name: &str,
    m: &Model,
    ix: &Index<'_>,
    dev: &BTreeSet<String>,
    t: &mut Tally,
) {
    let cfg = &m.config;
    for (i, g) in ix.go.iter().enumerate() {
        let Some(id) = ix.to_ours[i] else {
            continue;
        };
        let p = m.page(id);
        let at = || format!("{name} {} ({})", s(&g["path"]), s(&g["lang"]));
        let eq = |t: &mut Tally, check: &'static str, got: J, want: &J| {
            let d = diff(check, &got, want);
            t.check(check, d.is_none(), || {
                format!("{}: {}", at(), d.unwrap_or_default())
            });
        };
        eq(t, "title", json!(p.title), &g["title"]);
        eq(t, "linkTitle", json!(p.link_title), &g["linkTitle"]);
        eq(t, "type", json!(p.r#type), &g["type"]);
        eq(t, "section", json!(p.section), &g["section"]);
        if p.kind == PageKind::Term {
            let term = p
                .taxonomy
                .zip(p.term)
                .map(|(tx, tm)| m.sites[p.lang].taxonomies[tx].terms[tm].term.clone());
            eq(t, "term", json!(term.unwrap_or_default()), &g["term"]);
        }
        let d = &p.meta.dates;
        eq(
            t,
            "dates",
            json!({
                "date": time_json(d.date.as_ref()),
                "lastmod": time_json(d.lastmod.as_ref()),
                "publishDate": time_json(d.publish_date.as_ref()),
                "expiryDate": time_json(d.expiry_date.as_ref()),
            }),
            &g["dates"],
        );
        if p.source.is_none() {
            eq(
                t,
                "params (made pages)",
                to_json(&ssg_base::Value::map(p.meta.params.as_map().clone())),
                &g["params"],
            );
            eq(t, "build (made pages)", build_json(p), &g["build"]);
        }
        if p.role != PageRole::Standalone {
            continue;
        }
        eq(t, "parent", ix.opt(p.parent), &g["parent"]);
        eq(
            t,
            "currentSection",
            ix.go_ref(p.current_section),
            &g["currentSection"],
        );
        eq(
            t,
            "firstSection",
            ix.go_ref(p.first_section),
            &g["firstSection"],
        );
        if p.kind != PageKind::Page {
            let (got, want) = (ix.list(&p.sections), plain_list(&g["sections"]));
            let without_dev = |l: &J| {
                J::Array(
                    l.as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| {
                            e["p"].as_u64().is_none_or(|i| {
                                !dev.contains(s(&ix.go[usize::try_from(i).unwrap()]["path"]))
                            })
                        })
                        .cloned()
                        .collect(),
                )
            };
            if got != want && without_dev(&got) == without_dev(&want) {
                t.accept("sections", "segment-wise-taxonomy-prefix");
            } else {
                eq(t, "sections", got, &want);
            }
        }

        // Outputs: per rendered format, the file, link and resource directory.
        let want_out: BTreeMap<&str, &J> = g["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["render"] == true)
            .map(|o| (s(&o["name"]), &o["target"]))
            .collect();
        let got_out: BTreeMap<&str, J> = p
            .urls
            .iter()
            .filter(|_| p.rendered())
            .map(|u| {
                let res = u.paths.resources.as_ref();
                (
                    cfg.output_formats.get(u.format).name.as_str(),
                    json!({
                        "filename": u.paths.target.as_str(),
                        "link": u.paths.link.escaped(),
                        "subTarget": res.map_or("", |r| dir_str(r.target.as_str())),
                        "subLink": res.map_or("", |r| dir_str(r.link.as_str())),
                    }),
                )
            })
            .collect();
        let got_names: Vec<&str> = got_out.keys().copied().collect();
        let want_names: Vec<&str> = want_out.keys().copied().collect();
        eq(t, "rendered formats", json!(got_names), &json!(want_names));
        for (f, w) in &want_out {
            if let Some(g2) = got_out.get(f) {
                let mut w = (*w).clone();
                // Output paths are clean: Go's `/th/section/` resource directory is
                // `/th/section` (ssg-page's accepted deviation).
                if let Some(J::String(d)) = w.get_mut("subTarget")
                    && d.len() > 1
                    && d.ends_with('/')
                {
                    d.pop();
                }
                eq(t, "target paths", g2.clone(), &w);
            }
        }
        // `.OutputFormats`: name, rel and links (none without a link).
        let got_of: Vec<J> = p
            .urls
            .iter()
            .filter_map(|u| {
                let l = u.links.as_ref()?;
                let f = cfg.output_formats.get(u.format);
                // Go's `rel` (a view detail): `canonical` for a page's only format when it is
                // a built-in one.
                let builtin = ssg_config::OutputFormats::builtin(&cfg.media_types)
                    .by_name(&f.name)
                    .is_some();
                let rel = if p.urls.len() == 1 && builtin {
                    "canonical"
                } else {
                    f.rel.as_str()
                };
                Some(json!({
                    "name": f.name,
                    "rel": rel,
                    "relPermalink": l.rel_permalink.escaped(),
                    "permalink": l.permalink.to_string(),
                }))
            })
            .collect();
        eq(
            t,
            "output formats",
            J::Array(got_of),
            &g["pageOutputFormats"],
        );
    }
}
