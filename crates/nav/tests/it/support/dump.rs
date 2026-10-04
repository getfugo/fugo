//! The site of a structure dump, as the navigation model the tests run on.

use super::*;

/// A page of a fixture's page dump.
pub struct DumpPage {
    pub lang: LangIdx,
    pub kind: PageKind,
    pub lang_key: String,
    pub path: String,
    pub section: String,
    pub title: String,
    pub link_title: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub page_type: String,
    pub layout: String,
    pub bundle_type: String,
    pub draft: bool,
    pub weight: i32,
    pub keywords: Vec<String>,
    pub aliases: Vec<String>,
    pub dates: Dates,
    pub params: Params,
    pub menus: Vec<PageMenuEntry>,
    pub rel_permalink: String,
    pub fragments: Vec<String>,
    pub headings: Vec<String>,
    pub ancestor_of: Vec<usize>,
    pub outputs: Vec<(FormatId, TargetPaths)>,
    pub aliases_rendered: Rendering,
}

/// A [`NavModel`] over the `pages` dump of a fixture. `pageRef`s resolve by path in the same
/// language (the real `get_page` is T23b's).
pub struct DumpSite {
    pub pages: Vec<DumpPage>,
    pub langs: usize,
}

impl DumpSite {
    pub fn new(fx: &J) -> Self {
        let pages: Vec<DumpPage> = fx["pages"]
            .as_array()
            .expect("pages")
            .iter()
            .map(|p| {
                let params = p["params"]
                    .as_object()
                    .map(|_| Params::fold(value(&p["params"]).as_map().expect("map")))
                    .unwrap_or_default();
                DumpPage {
                    lang: LangIdx::from_index(idx(&p["site"])),
                    kind: PageKind::parse(s(&p["kind"])).expect("kind"),
                    lang_key: s(&p["lang"]).to_owned(),
                    path: s(&p["path"]).to_owned(),
                    section: s(&p["section"]).to_owned(),
                    title: s(&p["title"]).to_owned(),
                    link_title: s(&p["linkTitle"]).to_owned(),
                    name: s(&p["name"]).to_owned(),
                    slug: s(&p["slug"]).to_owned(),
                    description: s(&p["description"]).to_owned(),
                    page_type: s(&p["pageType"]).to_owned(),
                    layout: s(&p["layout"]).to_owned(),
                    bundle_type: s(&p["bundleType"]).to_owned(),
                    draft: p["draft"].as_bool().unwrap_or(false),
                    weight: i32::try_from(p["weight"].as_i64().unwrap_or(0)).expect("weight"),
                    keywords: strings(&p["keywords"]),
                    aliases: strings(&p["aliases"]),
                    dates: Dates {
                        date: zoned(&p["date"]),
                        lastmod: zoned(&p["lastmod"]),
                        publish_date: zoned(&p["publishDate"]),
                        expiry_date: zoned(&p["expiryDate"]),
                    },
                    menus: page_menus(&params).unwrap_or_default(),
                    params,
                    rel_permalink: s(&p["relPermalink"]).to_owned(),
                    fragments: strings(&p["fragments"]),
                    headings: strings(&p["headings"]),
                    ancestor_of: p["ancestorOf"]
                        .as_array()
                        .map(|a| a.iter().map(idx).collect())
                        .unwrap_or_default(),
                    outputs: Vec::new(),
                    aliases_rendered: Rendering::Rendered,
                }
            })
            .collect();
        let langs = fx["sites"].as_array().map_or(1, Vec::len);
        Self { pages, langs }
    }
}

impl NavModel for DumpSite {
    fn page(&self, id: PageId) -> PageFacts<'_> {
        let p = &self.pages[id.index()];
        PageFacts {
            id,
            lang: p.lang,
            kind: p.kind,
            lang_key: &p.lang_key,
            section: &p.section,
            title: &p.title,
            link_title: &p.link_title,
            name: &p.name,
            slug: &p.slug,
            description: &p.description,
            page_type: &p.page_type,
            layout: &p.layout,
            bundle_type: &p.bundle_type,
            draft: p.draft,
            weight: p.weight,
            keywords: &p.keywords,
            aliases: &p.aliases,
            dates: &p.dates,
            params: &p.params,
            menus: &p.menus,
            rel_permalink: &p.rel_permalink,
            fragments: &p.fragments,
            outputs: &p.outputs,
            rendering: p.aliases_rendered,
            list: ListMode::Always,
        }
    }

    fn tree_pages(&self, lang: LangIdx) -> Vec<PageId> {
        let mut ids: Vec<usize> = (0..self.pages.len())
            .filter(|&i| self.pages[i].lang == lang)
            .collect();
        let key = |i: &usize| self.pages[*i].path.trim_start_matches('/').to_owned();
        ids.sort_by_key(key);
        ids.into_iter().map(page_id).collect()
    }

    fn resolve_page_ref(&self, lang: LangIdx, reference: &str) -> Option<PageId> {
        let want = format!("/{}", reference.trim_matches('/')).to_lowercase();
        self.pages
            .iter()
            .position(|p| p.lang == lang && p.path.to_lowercase() == want)
            .map(page_id)
    }

    fn is_ancestor(&self, ancestor: PageId, page: PageId) -> bool {
        self.pages[ancestor.index()]
            .ancestor_of
            .contains(&page.index())
    }

    fn pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn regular_pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn site_regular_pages(&self, _: LangIdx) -> &[PageId] {
        &[]
    }

    fn home(&self, lang: LangIdx) -> Option<PageId> {
        self.pages
            .iter()
            .position(|p| p.lang == lang && p.kind == PageKind::Home)
            .map(page_id)
    }
}
