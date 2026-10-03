//! Menus: the configured entries, the section pages menu and the pages' own entries, assembled
//! into one tree per menu and language, and the "is this entry current" queries.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ssg_base::diag::Diagnostic;
use ssg_base::url::{LinkStyle, SiteUrls, add_context_root};
use ssg_base::{IdVec, LangIdx, PageId, PageKind, Params, text};
use ssg_config::Config;
use ssg_config::sections::MenuEntryConfig;
use ssg_page::ListMode;

use crate::model::{NavModel, PageFacts};

/// One entry of a menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuEntry {
    /// Empty: the entry is known by its name.
    pub identifier: String,
    pub name: String,
    pub title: String,
    /// `.URL`: the linked page's `.RelPermalink`, else the configured URL (made to match the
    /// site's links when it starts with `/`).
    pub url: String,
    /// The configured `pageRef`.
    pub page_ref: String,
    pub page: Option<PageId>,
    pub weight: i32,
    pub parent: Option<String>,
    /// HTML.
    pub pre: String,
    /// HTML.
    pub post: String,
    pub params: Params,
    /// Sorted with [`menu_order`].
    pub children: Vec<MenuEntry>,
}

impl MenuEntry {
    /// `.KeyName`: the identifier, else the name.
    #[must_use]
    pub fn key_name(&self) -> &str {
        if self.identifier.is_empty() {
            &self.name
        } else {
            &self.identifier
        }
    }

    /// `.HasChildren`.
    #[must_use]
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// The identifier, else the URL, else the name.
    fn unique_id(&self) -> &str {
        if !self.identifier.is_empty() {
            &self.identifier
        } else if !self.url.is_empty() {
            &self.url
        } else {
            &self.name
        }
    }

    /// The same entry: same identity (identifier, URL or name) below the same parent.
    fn same_entry(&self, other: &Self) -> bool {
        self.unique_id() == other.unique_id() && self.parent == other.parent
    }

    fn same_page(&self, page: Option<PageId>) -> bool {
        self.page.is_some() && self.page == page
    }

    /// The same page, or (for entries without one) the same non-empty URL.
    fn same_resource(&self, other: &Self) -> bool {
        if self.same_page(other.page) {
            return true;
        }
        !self.url.is_empty() && self.url == other.url
    }

    /// Fills what the configuration left empty from the linked page.
    fn link_page(&mut self, p: &PageFacts<'_>) {
        self.page = Some(p.id);
        p.rel_permalink.clone_into(&mut self.url);
        if self.name.is_empty() {
            p.link_title.clone_into(&mut self.name);
        }
        if self.title.is_empty() {
            p.title.clone_into(&mut self.title);
        }
        if self.weight == 0 {
            self.weight = p.weight;
        }
    }
}

/// Compares names case-insensitively (simple case folding), then byte-wise.
#[must_use]
pub fn compare_names(a: &str, b: &str) -> Ordering {
    a.chars()
        .map(text::lower_char)
        .cmp(b.chars().map(text::lower_char))
        .then_with(|| a.cmp(b))
}

/// The order of menu entries: weight ascending with 0 last, then name ([`compare_names`]),
/// then identifier. Use with a stable sort.
#[must_use]
pub fn menu_order(a: &MenuEntry, b: &MenuEntry) -> Ordering {
    if a.weight != b.weight {
        return match (a.weight, b.weight) {
            (_, 0) => Ordering::Less,
            (0, _) => Ordering::Greater,
            (x, y) => x.cmp(&y),
        };
    }
    compare_names(&a.name, &b.name).then_with(|| a.identifier.cmp(&b.identifier))
}

/// `.ByName`: entries by name only (stable).
pub fn sort_by_name(entries: &mut [MenuEntry]) {
    entries.sort_by(|a, b| compare_names(&a.name, &b.name));
}

/// The entries of one language's menus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteMenus {
    /// Menu name → top-level entries in [`menu_order`].
    pub menus: BTreeMap<String, Vec<MenuEntry>>,
    /// The pages' own entries (`.Menus` of a page) after assembly: menu name → entry.
    pub page_menus: BTreeMap<PageId, BTreeMap<String, MenuEntry>>,
}

impl SiteMenus {
    /// The top-level entries of menu `name`.
    #[must_use]
    pub fn menu(&self, name: &str) -> &[MenuEntry] {
        self.menus.get(name).map_or(&[], Vec::as_slice)
    }

    /// `.IsMenuCurrent menu entry` of page `page`: the page's own entry in `menu` is
    /// `entry`, or `entry` links to the page and some entry of `menu` (at any depth) is the
    /// same page or URL.
    #[must_use]
    pub fn is_menu_current(&self, page: PageId, menu: &str, entry: &MenuEntry) -> bool {
        if self
            .page_menus
            .get(&page)
            .and_then(|m| m.get(menu))
            .is_some_and(|own| own.same_entry(entry))
        {
            return true;
        }
        if !entry.same_page(Some(page)) {
            return false;
        }
        fn contains(entries: &[MenuEntry], entry: &MenuEntry) -> bool {
            entries
                .iter()
                .any(|e| e.same_resource(entry) || contains(&e.children, entry))
        }
        contains(self.menu(menu), entry)
    }

    /// `.HasMenuCurrent menu entry` of page `page`: `entry` links to a section above the
    /// page, or one of its descendants is the page's own entry or links to the page.
    #[must_use]
    pub fn has_menu_current(
        &self,
        m: &impl NavModel,
        page: PageId,
        menu: &str,
        entry: &MenuEntry,
    ) -> bool {
        if let Some(target) = entry.page
            && m.page(target).kind == PageKind::Section
            && m.is_ancestor(target, page)
        {
            return true;
        }
        if !entry.has_children() {
            return false;
        }
        if let Some(own) = self.page_menus.get(&page).and_then(|pm| pm.get(menu)) {
            for child in &entry.children {
                if child.same_entry(own) || self.has_menu_current(m, page, menu, child) {
                    return true;
                }
            }
        }
        entry
            .children
            .iter()
            .any(|child| child.same_page(Some(page)) || self.has_menu_current(m, page, menu, child))
    }
}

/// The menus of every language.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menus(pub IdVec<LangIdx, SiteMenus>);

/// What one language's menus are built from.
#[derive(Clone, Debug)]
pub struct MenuOptions<'a> {
    /// `[[menus.<name>]]` of the language.
    pub entries: &'a [MenuEntryConfig],
    /// `sectionPagesMenu`: the menu the top-level sections are added to.
    pub section_pages_menu: Option<String>,
    pub urls: SiteUrls,
}

impl<'a> MenuOptions<'a> {
    /// The options of language `lang` of `cfg`.
    #[must_use]
    pub fn from_config(cfg: &'a Config, lang: LangIdx) -> Self {
        let site = &cfg.sites[lang];
        Self {
            entries: &site.menus,
            section_pages_menu: site.section_pages_menu.clone(),
            urls: site.site_urls(),
        }
    }
}

/// A page's own menu entries (front matter `menus`), completed from the page: menu name →
/// entry.
#[must_use]
pub fn page_menu_entries(p: &PageFacts<'_>) -> BTreeMap<String, MenuEntry> {
    p.menus
        .iter()
        .map(|pm| {
            let mut e = MenuEntry {
                identifier: pm.identifier.clone(),
                name: pm.name.clone(),
                title: pm.title.clone(),
                pre: pm.pre.clone(),
                post: pm.post.clone(),
                parent: Some(pm.parent.clone()).filter(|s| !s.is_empty()),
                weight: pm.weight,
                params: pm.params.clone(),
                ..MenuEntry::default()
            };
            e.link_page(p);
            (pm.menu.clone(), e)
        })
        .collect()
}

/// A configured URL made to match the site's links: `/`-rooted URLs are urlized and get the
/// base URL's path (unless URLs are canonified); others are kept.
fn configured_url(url: &str, urls: &SiteUrls) -> String {
    if !url.starts_with('/') {
        return url.to_owned();
    }
    let u = urls.urlize(url);
    if urls.link_style == LinkStyle::Canonify {
        return u;
    }
    add_context_root(urls.base_url.as_str(), &u).map_or(u, |p| {
        String::from_utf8(p).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    })
}

fn config_entry(c: &MenuEntryConfig) -> MenuEntry {
    MenuEntry {
        identifier: c.identifier.clone(),
        name: c.name.clone(),
        title: c.title.clone(),
        url: c.url.clone(),
        page_ref: c.page_ref.clone(),
        page: None,
        weight: c.weight,
        parent: Some(c.parent.clone()).filter(|s| !s.is_empty()),
        pre: c.pre.clone(),
        post: c.post.clone(),
        params: c.params.clone(),
        children: Vec::new(),
    }
}

/// A page's own entry in one menu during assembly.
enum Own {
    /// Added to the menu: the node.
    Placed(usize),
    /// Dropped: the menu already had an entry with its key.
    Duplicate(Box<MenuEntry>),
}

/// The flat entries of one language before they form trees.
#[derive(Default)]
struct Assembly {
    /// Entries (without children) with the menu they were added to.
    nodes: Vec<(String, MenuEntry)>,
    /// (menu, key name) → node, and the keys in insertion order.
    flat: BTreeMap<(String, String), usize>,
    order: Vec<(String, String)>,
}

impl Assembly {
    fn push(&mut self, menu: &str, e: MenuEntry) -> usize {
        self.nodes.push((menu.to_owned(), e));
        self.nodes.len() - 1
    }

    /// Adds (or, for a configured duplicate, replaces) the entry at `key`.
    fn insert(&mut self, key: (String, String), node: usize) {
        if self.flat.insert(key.clone(), node).is_none() {
            self.order.push(key);
        }
    }
}

/// Builds the menus of language `lang`: configured entries (with `pageRef` resolved), the
/// section pages menu, then the pages' own entries in walk order; children go below their
/// parent (a missing parent is created with the parent's name and no URL). A page entry whose
/// menu already has an entry with its key is dropped with a warning.
pub fn build_site_menus(
    m: &impl NavModel,
    lang: LangIdx,
    o: &MenuOptions<'_>,
) -> (SiteMenus, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut a = Assembly::default();

    // Configured entries, per menu in name order, each menu in entry order.
    let mut configured: BTreeMap<&str, Vec<MenuEntry>> = BTreeMap::new();
    for c in o.entries {
        configured
            .entry(c.menu.as_str())
            .or_default()
            .push(config_entry(c));
    }
    for (menu, mut entries) in configured {
        entries.sort_by(menu_order);
        for mut e in entries {
            let page = (!e.page_ref.is_empty())
                .then(|| m.resolve_page_ref(lang, &e.page_ref))
                .flatten();
            match page {
                Some(id) => e.link_page(&m.page(id)),
                None => e.url = configured_url(&e.url, &o.urls),
            }
            let key = (menu.to_owned(), e.key_name().to_owned());
            let node = a.push(menu, e);
            a.insert(key, node);
        }
    }

    let listed: Vec<PageId> = m
        .tree_pages(lang)
        .into_iter()
        .filter(|&id| m.page(id).list == ListMode::Always)
        .collect();

    // The section pages menu: one entry per top-level section.
    if let Some(menu) = &o.section_pages_menu {
        for &id in &listed {
            let p = m.page(id);
            if p.kind != PageKind::Section {
                continue;
            }
            let ident = if p.section.is_empty() { "/" } else { p.section };
            let key = (menu.clone(), ident.to_owned());
            if a.flat.contains_key(&key) {
                continue;
            }
            let mut e = MenuEntry {
                identifier: ident.to_owned(),
                name: p.link_title.to_owned(),
                weight: p.weight,
                ..MenuEntry::default()
            };
            e.link_page(&p);
            let node = a.push(menu, e);
            a.insert(key, node);
        }
    }

    // The pages' own entries.
    let mut own: Vec<(PageId, BTreeMap<String, Own>)> = Vec::new();
    for &id in &listed {
        let p = m.page(id);
        if p.menus.is_empty() {
            continue;
        }
        let mut mine = BTreeMap::new();
        for (menu, e) in page_menu_entries(&p) {
            let key = (menu.clone(), e.key_name().to_owned());
            if a.flat.contains_key(&key) {
                diags.push(
                    Diagnostic::warning(format!(
                        "page {:?}: duplicate menu entry {:?} in menu {menu:?}",
                        p.rel_permalink,
                        e.key_name()
                    ))
                    .with_id("duplicate-menu-entry"),
                );
                mine.insert(menu, Own::Duplicate(Box::new(e)));
                continue;
            }
            let node = a.push(&menu, e);
            a.insert(key, node);
            mine.insert(menu, Own::Placed(node));
        }
        own.push((id, mine));
    }

    // Children below their parents.
    let mut children: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    let mut child_keys: Vec<(String, String)> = Vec::new();
    for key in &a.order {
        let node = a.flat[key];
        let (menu, e) = &a.nodes[node];
        if let Some(parent) = &e.parent {
            let ck = (menu.clone(), parent.clone());
            children
                .entry(ck.clone())
                .or_insert_with(|| {
                    child_keys.push(ck);
                    Vec::new()
                })
                .push(node);
        }
    }
    let mut child_lists: Vec<Vec<usize>> = vec![Vec::new(); a.nodes.len()];
    for ck in child_keys {
        let mut list = children.remove(&ck).unwrap_or_default();
        list.sort_by(|&x, &y| menu_order(&a.nodes[x].1, &a.nodes[y].1));
        let parent = if let Some(&p) = a.flat.get(&ck) {
            p
        } else {
            let e = MenuEntry {
                name: ck.1.clone(),
                ..MenuEntry::default()
            };
            let p = a.push(&ck.0, e);
            a.insert(ck, p);
            child_lists.push(Vec::new());
            p
        };
        child_lists[parent] = list;
    }

    // Top level, then freeze the trees.
    let mut top: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for key in &a.order {
        let node = a.flat[key];
        if a.nodes[node].1.parent.is_none() {
            top.entry(key.0.clone()).or_default().push(node);
        }
    }
    let mut frozen: Vec<Option<MenuEntry>> = vec![None; a.nodes.len()];
    let mut visiting = vec![false; a.nodes.len()];
    let mut menus = BTreeMap::new();
    for (name, mut nodes) in top {
        nodes.sort_by(|&x, &y| menu_order(&a.nodes[x].1, &a.nodes[y].1));
        let entries = nodes
            .into_iter()
            .map(|n| freeze(n, &a.nodes, &child_lists, &mut frozen, &mut visiting))
            .collect();
        menus.insert(name, entries);
    }
    let page_menus = own
        .into_iter()
        .map(|(id, mine)| {
            let entries = mine
                .into_iter()
                .map(|(menu, e)| {
                    let e = match e {
                        Own::Placed(n) => {
                            freeze(n, &a.nodes, &child_lists, &mut frozen, &mut visiting)
                        }
                        Own::Duplicate(e) => *e,
                    };
                    (menu, e)
                })
                .collect();
            (id, entries)
        })
        .collect();
    (SiteMenus { menus, page_menus }, diags)
}

/// The entry `n` with its children; an entry below itself (a parent cycle) loses the edge that
/// closes the cycle.
fn freeze(
    n: usize,
    nodes: &[(String, MenuEntry)],
    children: &[Vec<usize>],
    frozen: &mut [Option<MenuEntry>],
    visiting: &mut [bool],
) -> MenuEntry {
    if let Some(e) = &frozen[n] {
        return e.clone();
    }
    visiting[n] = true;
    let mut e = nodes[n].1.clone();
    for &c in &children[n] {
        if !visiting[c] {
            let child = freeze(c, nodes, children, frozen, visiting);
            e.children.push(child);
        }
    }
    visiting[n] = false;
    frozen[n] = Some(e.clone());
    e
}

/// Builds the menus of every language of `cfg`.
pub fn build_menus(m: &impl NavModel, cfg: &Config) -> (Menus, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut out = IdVec::with_capacity(cfg.sites.len());
    for lang in cfg.sites.ids() {
        let (menus, d) = build_site_menus(m, lang, &MenuOptions::from_config(cfg, lang));
        diags.extend(d);
        out.push(menus);
    }
    (Menus(out), diags)
}
