//! Decoding the site's configuration: its keys, taxonomies and disabled kinds.

use super::*;

pub(crate) fn decode_site(tree: &Map, cx: SiteContext<'_>) -> Result<SiteConfig, ConfigError> {
    let raw: Raw = crate::de::from_map(tree).map_err(|e| crate::decode_error("", &e))?;
    let section = |k: &str| {
        tree.get(k)
            .and_then(Value::as_map)
            .cloned()
            .unwrap_or_default()
    };

    let time_zone = if raw.time_zone.is_empty() {
        jiff::tz::TimeZone::UTC
    } else {
        jiff::tz::TimeZone::get(&raw.time_zone).map_err(|e| {
            ConfigError::invalid(
                "timeZone",
                format_args!("unknown time zone {:?}: {e}", raw.time_zone),
            )
        })?
    };
    let language = Language {
        idx: cx.lang,
        key: cx.key.to_owned(),
        name: raw.language_name,
        code: if raw.language_code.is_empty() {
            cx.key.to_owned()
        } else {
            raw.language_code
        },
        direction: if raw.language_direction.eq_ignore_ascii_case("rtl") {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        weight: raw.weight,
        time_zone,
        url_prefix: cx.url_prefix,
        title: cx
            .own
            .get("title")
            .and_then(crate::de::weak_string)
            .unwrap_or_default(),
    };
    let base_url = BaseUrl::parse(&raw.base_url).map_err(|e| ConfigError::invalid("baseURL", e))?;

    let taxonomies: IdVec<TaxonomyIdx, TaxonomyDef> = match tree.get("taxonomies") {
        None => vec![("category", "categories"), ("tag", "tags")]
            .into_iter()
            .map(|(s, p)| TaxonomyDef {
                singular: s.to_owned(),
                plural: p.to_owned(),
                hierarchical: false,
            })
            .collect(),
        Some(Value::Map(m)) => {
            let mut defs = IdVec::new();
            for (k, v) in m.iter().filter(|(k, _)| *k != "_merge") {
                if let Some(def) = taxonomy_def(k, v)? {
                    defs.push(def);
                }
            }
            defs
        }
        Some(_) => return Err(ConfigError::invalid("taxonomies", "expected a table")),
    };

    let (disable_kinds, rss_disabled) = disabled_kinds(tree.get("disablekinds"), cx.diagnostics);
    let outputs = KindOutputs::decode(
        &section("outputs"),
        cx.output_formats,
        disable_kinds,
        rss_disabled,
    )?;
    let permalinks = Permalinks::decode(tree.get("permalinks").unwrap_or(&Value::Null))?;

    let mut markup = raw.markup;
    for hook in [
        &mut markup.goldmark.render_hooks.image,
        &mut markup.goldmark.render_hooks.link,
    ] {
        hook.use_embedded.get_or_insert(cx.use_embedded);
    }

    let related = match tree.get("related") {
        None => RelatedConfig::default_for(cx.default_has_tags),
        Some(Value::Map(m)) => {
            RelatedConfig::decode(m).map_err(|e| crate::decode_error("related", &e))?
        }
        Some(_) => return Err(ConfigError::invalid("related", "expected a table")),
    };
    let menus = match tree.get("menus") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Map(m)) => {
            crate::sections::decode_menus(m).map_err(|e| crate::decode_error("menus", &e))?
        }
        Some(_) => return Err(ConfigError::invalid("menus", "expected a table")),
    };
    let cascade = crate::sections::decode_cascade(tree.get("cascade").unwrap_or(&Value::Null))
        .map_err(|e| crate::decode_error("cascade", &e))?;

    let params = tree
        .get("params")
        .and_then(Value::as_map)
        .map(Params::fold)
        .unwrap_or_default();
    let main_sections = tree
        .get("mainsections")
        .or_else(|| params.get("mainsections"))
        .map(|v| match v {
            Value::Array(a) => a.iter().filter_map(crate::de::weak_string).collect(),
            other => crate::de::weak_string(other).into_iter().collect(),
        });

    let summary_length = usize::try_from(raw.summary_length.unwrap_or(70))
        .map_err(|_| ConfigError::invalid("summaryLength", "must not be negative"))?;

    Ok(SiteConfig {
        lang: cx.lang,
        language,
        base_url,
        title: raw.title,
        copyright: raw.copyright,
        params,
        taxonomies,
        outputs,
        permalinks,
        pagination: raw.pagination,
        markup,
        front_matter: crate::sections::decode_front_matter(&section("frontmatter")),
        related,
        sitemap: raw.sitemap,
        services: raw.services,
        menus,
        section_pages_menu: Some(raw.section_pages_menu).filter(|s| !s.is_empty()),
        cascade,
        urls: UrlPolicy {
            link_style: if raw.canonify_urls {
                LinkStyle::Canonify
            } else {
                LinkStyle::Relative
            },
            output: if raw.relative_urls {
                LinkOutput::Relative
            } else {
                LinkOutput::AsRendered
            },
            ugly: UglyUrls::decode(tree.get("uglyurls")),
            path_case: if raw.disable_path_to_lower {
                PathCase::Preserve
            } else {
                PathCase::Lower
            },
            accents: if raw.remove_path_accents {
                Accents::Remove
            } else {
                Accents::Keep
            },
        },
        disable_kinds,
        aliases: if raw.disable_aliases {
            AliasPolicy::Disabled
        } else {
            AliasPolicy::Write
        },
        robots_txt: if raw.enable_robots_txt {
            RobotsPolicy::Enabled
        } else {
            RobotsPolicy::Disabled
        },
        emoji: if raw.enable_emoji {
            EmojiPolicy::Enabled
        } else {
            EmojiPolicy::Disabled
        },
        titles: TitleConfig {
            case_style: title::Style::parse(&raw.title_case_style),
            pluralize: raw.pluralize_list_titles.unwrap_or(true),
            capitalize: raw.capitalize_list_titles.unwrap_or(true),
        },
        summary_length,
        content_dir: cx
            .own
            .get("contentdir")
            .and_then(crate::de::weak_string)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        static_dirs: cx.own.get("staticdir").map(|v| match v {
            Value::Array(a) => a
                .iter()
                .filter_map(crate::de::weak_string)
                .map(PathBuf::from)
                .collect(),
            other => crate::de::weak_string(other)
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        }),
        ref_links: RefLinksConfig {
            level: if raw.ref_links_error_level.eq_ignore_ascii_case("warning") {
                RefLinksLevel::Warning
            } else {
                RefLinksLevel::Error
            },
            not_found_url: raw.ref_links_not_found_url,
        },
        main_sections,
        has_cjk_language: raw.has_cjk_language,
    })
}

/// One entry of `[taxonomies]`: `tag = "tags"` or `[taxonomies.tag]` with `plural` and
/// `hierarchical`; `None` for an empty plural (the taxonomy is switched off).
pub(super) fn taxonomy_def(singular: &str, v: &Value) -> Result<Option<TaxonomyDef>, ConfigError> {
    let (plural, hierarchical) = match v {
        Value::Map(t) => {
            let key = |k: &str| format!("taxonomies.{singular}.{k}");
            if let Some(other) = t
                .keys()
                .find(|k| !matches!(*k, "plural" | "hierarchical" | "_merge"))
            {
                return Err(ConfigError::invalid(key(other), "unknown key"));
            }
            let plural = match t.get("plural") {
                None => String::new(),
                Some(p) => crate::de::weak_string(p)
                    .ok_or_else(|| ConfigError::invalid(key("plural"), "expected a string"))?,
            };
            let hierarchical = match t.get("hierarchical") {
                None => false,
                Some(h) => crate::de::weak_bool(h).ok_or_else(|| {
                    ConfigError::invalid(key("hierarchical"), "expected a boolean")
                })?,
            };
            (plural, hierarchical)
        }
        other => match crate::de::weak_string(other) {
            Some(p) => (p, false),
            None => return Ok(None),
        },
    };
    Ok((!plural.is_empty()).then(|| TaxonomyDef {
        singular: singular.to_owned(),
        plural,
        hierarchical,
    }))
}

/// `disableKinds`: the disabled page kinds, and whether `rss` is disabled. Unknown kinds are
/// reported and ignored.
pub(super) fn disabled_kinds(
    v: Option<&Value>,
    diagnostics: &mut Vec<ssg_base::diag::Diagnostic>,
) -> (KindSet, bool) {
    let mut set = KindSet::EMPTY;
    let mut rss = false;
    let Some(Value::Array(items)) = v.map(crate::tree::split_list) else {
        return (set, rss);
    };
    for item in items.iter().filter_map(crate::de::weak_string) {
        let lower = item.to_ascii_lowercase();
        if lower == "rss" {
            rss = true;
            continue;
        }
        if lower == "taxonomyterm" {
            diagnostics.push(
                ssg_base::diag::Diagnostic::warning(
                    "disableKinds: the kind \"taxonomyTerm\" is deprecated; use \"taxonomy\"",
                )
                .with_id("deprecated-disablekinds-taxonomyterm"),
            );
        }
        match PageKind::parse(&lower) {
            Some(k) => set.insert(k),
            None => diagnostics.push(ssg_base::diag::Diagnostic::warning(format!(
                "disableKinds: unknown kind {item:?}"
            ))),
        }
    }
    (set, rss)
}
