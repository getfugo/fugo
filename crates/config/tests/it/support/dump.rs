//! The projection of a `Config` onto the oracle's `config` dumps.

use super::*;

/// Removes zero values (`null`, `false`, `0`, `""`, empty lists and tables) recursively and
/// writes numbers as floats, so dumps with and without zero values compare equal.
pub fn strip(v: &J) -> Option<J> {
    match v {
        J::Null => None,
        J::Bool(false) => None,
        J::Bool(true) => Some(J::Bool(true)),
        J::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            (f != 0.0).then(|| json!(f))
        }
        J::String(s) => (!s.is_empty()).then(|| J::String(s.clone())),
        J::Array(a) => {
            let items: Vec<J> = a.iter().map(|x| strip(x).unwrap_or(J::Null)).collect();
            (!items.is_empty()).then_some(J::Array(items))
        }
        J::Object(o) => {
            let m: JMap<String, J> = o
                .iter()
                .filter_map(|(k, v)| Some((k.to_lowercase(), strip(v)?)))
                .collect();
            (!m.is_empty()).then_some(J::Object(m))
        }
    }
}

pub(super) fn map(m: &ssg_base::Map) -> J {
    serde_json::to_value(m).expect("json")
}

pub(super) fn nanos(a: MaxAge) -> J {
    match a {
        MaxAge::Forever => json!(-1),
        MaxAge::For(d) => json!(d.as_nanos() as f64),
    }
}

/// The projection of `site` (and the project-wide settings of `c`) onto the keys of the
/// oracle's per-language dump that this crate types.
#[allow(clippy::too_many_lines)]
pub fn dump(c: &Config, s: &SiteConfig) -> JMap<String, J> {
    let mut d = JMap::new();
    let mut put = |k: &str, v: J| {
        d.insert(k.to_owned(), v);
    };
    put("title", json!(s.title));
    put("copyright", json!(s.copyright));
    put("params", map(s.params.as_map()));
    put(
        "taxonomies",
        J::Object(
            s.taxonomies
                .iter()
                .map(|t| (t.singular.clone(), json!(t.plural)))
                .collect(),
        ),
    );
    put(
        "outputformats",
        J::Object(
            c.output_formats
                .iter()
                .map(|(_, f)| {
                    (
                        f.name.clone(),
                        json!({
                            "mediatype": c.media_types.get(f.media_type).to_string(),
                            "basename": f.base_name, "path": f.path, "rel": f.rel,
                            "protocol": f.protocol,
                            "isplaintext": f.escaping == Escaping::Plain,
                            "ishtml": f.is_html,
                            "nougly": f.ugly == UglyPolicy::Never,
                            "ugly": f.ugly == UglyPolicy::Always,
                            "notalternative": f.listing == Listing::NotAlternative,
                            "root": f.placement == Placement::Root,
                            "permalinkable": f.links == LinkPolicy::Own,
                            "weight": f.weight,
                        }),
                    )
                })
                .collect(),
        ),
    );
    put(
        "mediatypes",
        J::Object(
            c.media_types
                .iter()
                .map(|(_, t)| {
                    (
                        t.to_string(),
                        json!({"suffixes": t.suffixes, "delimiter": t.delimiter}),
                    )
                })
                .collect(),
        ),
    );
    put(
        "contenttypes",
        J::Object(
            c.content_types
                .0
                .iter()
                .map(|&id| (c.media_types.get(id).to_string(), json!({})))
                .collect(),
        ),
    );
    put(
        "permalinks",
        J::Object(
            ssg_config::sections::PERMALINK_KINDS
                .iter()
                .map(|&k| {
                    (
                        k.as_str().to_owned(),
                        serde_json::to_value(s.permalinks.of_kind(k)).expect("json"),
                    )
                })
                .collect(),
        ),
    );
    put(
        "pagination",
        json!({"pagersize": s.pagination.pager_size, "path": s.pagination.path,
               "disablealiases": s.pagination.disable_aliases}),
    );
    put("markup", markup(s));
    put(
        "frontmatter",
        J::Object(
            s.front_matter
                .iter()
                .map(|(f, srcs)| {
                    (
                        f.key().to_owned(),
                        J::Array(srcs.iter().map(|x| json!(x.as_config_str())).collect()),
                    )
                })
                .collect(),
        ),
    );
    put(
        "related",
        json!({
            "threshold": s.related.threshold, "includenewer": s.related.include_newer,
            "tolower": s.related.to_lower,
            "indices": s.related.indices.iter().map(|i| json!({
                "name": i.name, "type": i.kind, "weight": i.weight,
                "cardinalitythreshold": i.cardinality_threshold, "pattern": i.pattern,
                "tolower": i.to_lower, "applyfilter": i.apply_filter,
            })).collect::<Vec<_>>(),
        }),
    );
    put("sitemap", serde_json::to_value(&s.sitemap).expect("json"));
    put("services", serde_json::to_value(&s.services).expect("json"));
    put("privacy", serde_json::to_value(&c.privacy).expect("json"));
    put(
        "security",
        json!({
            "enableinlineshortcodes": c.security.inline_shortcodes
                == ssg_config::global::InlineShortcodes::Enabled,
            "exec": {"allow": exec_allow_as_go(&c.security.exec_allow), "osenv": wl(&c.security.exec_os_env)},
            "funcs": {"getenv": getenv_as_go(&c.security.getenv)},
            "http": {"urls": wl(&c.security.http_urls), "methods": wl(&c.security.http_methods),
                     "mediatypes": wl(&c.security.http_media_types)},
        }),
    );
    put("build", build_as_go(&c.build));
    put(
        "caches",
        J::Object(
            c.caches
                .caches
                .iter()
                .map(|(k, v)| (k.clone(), json!({"dir": v.dir, "maxage": nanos(v.max_age)})))
                .collect(),
        ),
    );
    let mut imaging = serde_json::to_value(&c.imaging).expect("json");
    imaging["exif"] = map(&c.imaging.exif);
    put("imaging", imaging);
    let mut minify = JMap::new();
    minify.insert("minifyoutput".into(), json!(c.minify.minify_output));
    for t in &c.minify.disabled {
        minify.insert(
            format!(
                "disable{}",
                serde_json::to_value(t)
                    .expect("json")
                    .as_str()
                    .unwrap_or_default()
            ),
            json!(true),
        );
    }
    // `[minify.tdewolff]` defaults belong to the minify crate (T26); only the switches are
    // compared.
    put("minify", J::Object(minify));
    let mut menus = JMap::new();
    for e in &s.menus {
        let entry = json!({
            "identifier": e.identifier, "name": e.name, "pre": e.pre, "post": e.post,
            "url": e.url, "pageref": e.page_ref, "weight": e.weight, "parent": e.parent,
            "title": e.title, "params": map(e.params.as_map()),
        });
        menus
            .entry(e.menu.clone())
            .or_insert_with(|| J::Array(Vec::new()))
            .as_array_mut()
            .expect("list")
            .push(entry);
    }
    put("menus", J::Object(menus));
    put(
        "cascade",
        J::Array(
            s.cascade
                .iter()
                .map(|x| {
                    json!({"params": map(x.params.as_map()), "fields": map(x.fields.as_map()),
                           "target": x.target})
                })
                .collect(),
        ),
    );
    put("summarylength", json!(s.summary_length));
    put("pluralizelisttitles", json!(s.titles.pluralize));
    put("capitalizelisttitles", json!(s.titles.capitalize));
    put(
        "disablealiases",
        json!(s.aliases == ssg_config::site::AliasPolicy::Disabled),
    );
    put(
        "enableemoji",
        json!(s.emoji == ssg_config::site::EmojiPolicy::Enabled),
    );
    put(
        "enablerobotstxt",
        json!(s.robots_txt == ssg_config::site::RobotsPolicy::Enabled),
    );
    put(
        "canonifyurls",
        json!(s.urls.link_style == ssg_base::url::LinkStyle::Canonify),
    );
    put(
        "relativeurls",
        json!(s.urls.output == ssg_config::site::LinkOutput::Relative),
    );
    put(
        "removepathaccents",
        json!(s.urls.accents == ssg_base::url::Accents::Remove),
    );
    put(
        "disablepathtolower",
        json!(s.urls.path_case == ssg_base::url::PathCase::Preserve),
    );
    put(
        "uglyurls",
        match &s.urls.ugly {
            UglyUrls::Never => json!(false),
            UglyUrls::Always => json!(true),
            UglyUrls::Sections(m) => json!(m),
        },
    );
    put("hascjklanguage", json!(s.has_cjk_language));
    put(
        "reflinkserrorlevel",
        json!(match s.ref_links.level {
            ssg_config::site::RefLinksLevel::Error => "",
            ssg_config::site::RefLinksLevel::Warning => "WARNING",
        }),
    );
    put("reflinksnotfoundurl", json!(s.ref_links.not_found_url));
    put("mainsections", json!(s.main_sections));
    put("builddrafts", json!(c.content.drafts));
    put("buildfuture", json!(c.content.future));
    put("buildexpired", json!(c.content.expired));
    put("enablegitinfo", json!(c.enable_git_info));
    put("ignorelogs", json!(c.ignore_logs));
    put("ignorefiles", json!(c.ignore_files));
    put("defaultoutputformat", json!(c.default_output_format));
    put(
        "contentdir",
        json!(s.content_dir.as_ref().unwrap_or(&c.dirs.content)),
    );
    put("datadir", json!(c.dirs.data));
    put("layoutdir", json!(c.dirs.layouts));
    put("i18ndir", json!(c.dirs.i18n));
    put("archetypedir", json!(c.dirs.archetypes));
    put("assetdir", json!(c.dirs.assets));
    put("resourcedir", json!(c.dirs.resources));
    put("publishdir", json!(c.dirs.publish));
    put("themesdir", json!(c.dirs.themes));
    put(
        "staticdir",
        json!(s.static_dirs.as_ref().unwrap_or(&c.dirs.static_dirs)),
    );
    put("cachedir", json!(c.cache_dir));
    put(
        "defaultcontentlanguageinsubdir",
        json!(c.default_language_in_subdir),
    );
    put("environment", json!(c.environment));
    d
}
