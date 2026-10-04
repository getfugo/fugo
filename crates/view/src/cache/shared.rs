//! What the generations of a build share: the inputs turned into values once (dates, links, files,
//! menus, output formats).

use super::*;

/// The value of a serialisable view, as a map (views are structs).
pub(super) fn to_map<T: Serialize>(v: &T) -> tera::Map {
    tera::Value::from_serializable(v)
        .into_map()
        .unwrap_or_default()
}

/// The link value of page `id`.
pub(super) fn page_link(model: &Model, id: PageId) -> PageLink {
    let p = &model.pages[id];
    let (permalink, rel_permalink) = p.links().map_or_else(Default::default, |l| {
        (l.permalink.to_string(), l.rel_permalink.escaped())
    });
    PageLink {
        id: id.raw(),
        kind: p.kind,
        path: p.path(),
        lang: model.config.sites[p.lang].language.key.clone(),
        title: p.title.clone(),
        link_title: p.link_title.clone(),
        permalink,
        rel_permalink,
    }
}

pub(super) fn file_view(p: &ssg_site::Page) -> Option<FileView> {
    let src = p.source.as_ref()?;
    let rel = &src.file.rel;
    let (dir, file) = rel
        .rsplit_once('/')
        .map_or(("", rel.as_str()), |(d, f)| (d, f));
    let base = file.rsplit_once('.').map_or(file, |(b, _)| b);
    let unique_id = {
        use md5::Digest;
        md5::Md5::digest(rel.as_bytes())
            .iter()
            .fold(String::with_capacity(32), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
    };
    Some(FileView {
        path: rel.clone(),
        dir: if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        },
        base_file_name: base.to_owned(),
        content_base_name: src.file_info.original.name.clone(),
        unique_id,
        is_content_adapter: src.file_info.kind == ssg_vfs::BundleKind::ContentAdapter,
    })
}

pub(super) fn output_formats(
    model: &Model,
    media_types: &IdVec<FormatId, tera::Value>,
    p: &ssg_site::Page,
) -> tera::Value {
    let cfg = &model.config;
    let mut m = tera::Map::with_capacity(p.urls.len());
    for u in &p.urls {
        let Some(l) = &u.links else { continue };
        let f = cfg.output_formats.get(u.format);
        let v = OutputFormatView {
            name: f.name.clone(),
            rel: f.rel.clone(),
            media_type: media_types[u.format].clone(),
            permalink: l.permalink.to_string(),
            rel_permalink: l.rel_permalink.escaped(),
            is_plain_text: f.escaping == Escaping::Plain,
            is_html: f.is_html,
        };
        m.insert(f.name.clone().into(), tera::Value::from_serializable(&v));
    }
    tera::Value::from(m)
}

pub(super) fn menu_entry(model: &Model, e: &MenuEntry) -> MenuEntryView {
    MenuEntryView {
        identifier: e.identifier.clone(),
        key_name: e.key_name().to_owned(),
        name: e.name.clone(),
        title: e.title.clone(),
        url: e.url.clone(),
        weight: e.weight,
        parent: e.parent.clone(),
        pre: tera::Value::safe_string(&e.pre),
        post: tera::Value::safe_string(&e.post),
        params: params_value(&e.params),
        page: e.page.map(|p| page_link(model, p)),
        children: e.children.iter().map(|c| menu_entry(model, c)).collect(),
        has_children: e.has_children(),
    }
}

/// A list of strings; the shared `empty` value when there are none.
pub(super) fn strings(empty: &tera::Value, s: &[String]) -> tera::Value {
    if s.is_empty() {
        empty.clone()
    } else {
        tera::Value::from_serializable(s)
    }
}

/// A map of `a`'s entries followed by `b`'s, allocated once.
pub(super) fn merged(a: &tera::Value, b: tera::Map) -> tera::Value {
    let a = a.as_map();
    let mut m = tera::Map::with_capacity(a.map_or(0, tera::Map::len) + b.len());
    if let Some(a) = a {
        m.extend(a.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    m.extend(b);
    tera::Value::from(m)
}

/// Values many pages share, serialised once: dates repeated within a page, sitemap settings.
#[derive(Default)]
pub(super) struct Interner {
    pub(super) sitemaps: std::sync::Mutex<Vec<(String, u64, bool, tera::Value)>>,
}

impl Interner {
    pub(super) fn sitemap(&self, s: &ssg_config::sections::SitemapConfig) -> tera::Value {
        let mut all = self
            .sitemaps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (s.change_freq.as_str(), s.priority.to_bits(), s.disable);
        if let Some(v) = all
            .iter()
            .find(|(c, p, d, _)| (c.as_str(), *p, *d) == key)
            .map(|e| e.3.clone())
        {
            return v;
        }
        let v = tera::Value::from_serializable(&SitemapView {
            change_freq: s.change_freq.clone(),
            priority: s.priority,
            disable: s.disable,
        });
        all.push((s.change_freq.clone(), key.1, s.disable, v.clone()));
        v
    }
}

/// The four dates of a page; equal instants share one value.
pub(super) fn dates(d: &ssg_page::Dates) -> [tera::Value; 4] {
    let all = [&d.date, &d.lastmod, &d.publish_date, &d.expiry_date];
    let mut out: [tera::Value; 4] = std::array::from_fn(|_| tera::Value::none());
    for i in 0..4 {
        let Some(z) = all[i] else { continue };
        out[i] = (0..i).find(|&j| all[j].as_ref() == Some(z)).map_or_else(
            || tera::Value::from_serializable(&DateView::new(z)),
            |j| out[j].clone(),
        );
    }
    out
}

/// `(prev, next)` in `list` (newest first): prev is the older neighbour.
pub(super) fn neighbours(
    out: &mut IdVec<PageId, (Option<PageId>, Option<PageId>)>,
    list: &[PageId],
) {
    for (i, &q) in list.iter().enumerate() {
        out[q] = (list.get(i + 1).copied(), i.checked_sub(1).map(|j| list[j]));
    }
}

impl Shared {
    pub(super) fn new(inputs: ViewInputs) -> Result<Self, ViewError> {
        let ViewInputs {
            model,
            store,
            menus,
        } = inputs;
        let cfg = &model.config;
        let nav = NavSite::new(Arc::clone(&model));

        let resources = page_resources(&model, &store)?;
        let no_resources = tera::Value::from(Vec::<tera::Value>::new());
        let resource_values: Vec<tera::Value> = resources
            .iter()
            .map(|rs| {
                if rs.is_empty() {
                    return no_resources.clone();
                }
                tera::Value::from(
                    rs.iter()
                        .map(|r| tera::Value::from_serializable(&r.view))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let resources: IdVec<PageId, Vec<ResourceId>> = resources
            .into_iter()
            .map(|rs| rs.into_iter().map(|r| r.id).collect())
            .collect();

        let languages: IdVec<LangIdx, tera::Value> = cfg
            .sites
            .iter()
            .map(|s| tera::Value::from_serializable(&LanguageView::new(s)))
            .collect();
        let links: IdVec<PageId, tera::Value> = model
            .pages
            .ids()
            .map(|id| tera::Value::from_serializable(&page_link(&model, id)))
            .collect();

        let media_types: IdVec<FormatId, tera::Value> = cfg
            .output_formats
            .iter()
            .map(|(_, f)| {
                tera::Value::from_serializable(&MediaTypeView::new(
                    cfg.media_types.get(f.media_type),
                ))
            })
            .collect();
        let empty = tera::Value::from(Vec::<tera::Value>::new());
        let interner = Interner::default();
        let base: IdVec<PageId, tera::Value> = model
            .pages
            .as_slice()
            .par_iter()
            .zip(resource_values)
            .map(|(p, resources)| {
                let raw_content = tera::Value::from(p.source.as_ref().map_or("", |s| s.body()));
                let m = &p.meta;
                let [date, lastmod, publish_date, expiry_date] = dates(&m.dates);
                let site = &model.sites[p.lang];
                let mut terms = tera::Map::with_capacity(site.taxonomies.len());
                for (tx, t) in site.taxonomies.iter_enumerated() {
                    let links: Vec<tera::Value> = p
                        .terms
                        .iter()
                        .filter(|&&(x, _)| x == tx)
                        .map(|&(_, term)| links[t.terms[term].page].clone())
                        .collect();
                    let list = if links.is_empty() {
                        empty.clone()
                    } else {
                        tera::Value::from(links)
                    };
                    terms.insert(t.def.plural.clone().into(), list);
                }
                let (permalink, rel_permalink) = p.links().map_or_else(Default::default, |l| {
                    (l.permalink.to_string(), l.rel_permalink.escaped())
                });
                let bt = bundle_type(p);
                tera::Value::from_serializable(&PageSummaryView {
                    id: p.id.raw(),
                    kind: p.kind,
                    lang: cfg.sites[p.lang].language.key.clone(),
                    path: p.path(),
                    section: p.section.clone(),
                    r#type: p.r#type.clone(),
                    layout: m.layout.clone(),
                    bundle_type: (!bt.is_empty()).then_some(bt),
                    name: model.page_name(p.id).to_owned(),
                    title: p.title.clone(),
                    link_title: p.link_title.clone(),
                    description: m.description.clone(),
                    date,
                    lastmod,
                    publish_date,
                    expiry_date,
                    weight: m.weight,
                    draft: m.draft,
                    params: params_value(&m.params),
                    keywords: strings(&empty, &m.keywords),
                    aliases: strings(&empty, &m.aliases),
                    permalink,
                    rel_permalink,
                    is_home: p.kind == PageKind::Home,
                    is_section: p.kind == PageKind::Section,
                    is_page: p.kind == PageKind::Page,
                    is_node: p.kind.is_branch() || p.kind == PageKind::NotFound,
                    is_translated: p.translations.len() > 1,
                    file: file_view(p),
                    git_info: None,
                    sitemap: interner.sitemap(&m.sitemap),
                    language: languages[p.lang].clone(),
                    output_formats: output_formats(&model, &media_types, p),
                    resources,
                    terms: tera::Value::from(terms),
                    raw_content,
                })
            })
            .collect::<Vec<_>>()
            .into();

        let mut prev_next = model.pages.iter().map(|_| (None, None)).collect();
        for s in &model.sites {
            neighbours(&mut prev_next, &s.regular_pages);
        }
        let mut prev_next_in_section = model.pages.iter().map(|_| (None, None)).collect();
        for p in &model.pages {
            if matches!(p.kind, PageKind::Section | PageKind::Home) {
                neighbours(&mut prev_next_in_section, &p.regular_pages);
            }
        }

        let all_pages = model
            .sites
            .iter()
            .flat_map(|s| s.pages.iter().copied())
            .collect();
        let all_languages = tera::Value::from(languages.as_slice());
        let data = ssg_base::Value::Map(Arc::clone(&model.data)).to_tera();
        let configs = cfg
            .sites
            .iter()
            .map(|s| tera::Value::from_serializable(&SiteConfigView::new(cfg, s)))
            .collect();
        let menu_values = cfg
            .sites
            .ids()
            .map(|l| {
                let mut m = tera::Map::new();
                if let Some(sm) = menus.0.get(l) {
                    for (name, entries) in &sm.menus {
                        let list: Vec<MenuEntryView> =
                            entries.iter().map(|e| menu_entry(&model, e)).collect();
                        m.insert(name.clone().into(), tera::Value::from_serializable(&list));
                    }
                }
                tera::Value::from(m)
            })
            .collect();
        let sitemap_abs_urls = cfg
            .sites
            .ids()
            .map(|l| {
                model
                    .pages
                    .iter()
                    .find(|p| p.lang == l && p.kind == PageKind::Sitemap)
                    .and_then(|p| p.links())
                    .map(|l| l.permalink.to_string())
            })
            .collect();

        Ok(Self {
            model,
            nav,
            store,
            menus,
            resources,
            base,
            empty,
            links,
            prev_next,
            prev_next_in_section,
            all_pages,
            languages,
            all_languages,
            data,
            configs,
            menu_values,
            sitemap_abs_urls,
        })
    }
}
