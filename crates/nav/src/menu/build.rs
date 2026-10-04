//! Building the menus of a site: configured entries and pages' entries, assembled into trees, then
//! frozen.

use super::*;

/// A page's own entry in one menu during assembly.
pub(super) enum Own {
    /// Added to the menu: the node.
    Placed(usize),
    /// Dropped: the menu already had an entry with its key.
    Duplicate(Box<MenuEntry>),
}

/// The flat entries of one language before they form trees.
#[derive(Default)]
pub(super) struct Assembly {
    /// Entries (without children) with the menu they were added to.
    pub(super) nodes: Vec<(String, MenuEntry)>,
    /// (menu, key name) → node, and the keys in insertion order.
    pub(super) flat: BTreeMap<(String, String), usize>,
    pub(super) order: Vec<(String, String)>,
}

impl Assembly {
    pub(super) fn push(&mut self, menu: &str, e: MenuEntry) -> usize {
        self.nodes.push((menu.to_owned(), e));
        self.nodes.len() - 1
    }

    /// Adds (or, for a configured duplicate, replaces) the entry at `key`.
    pub(super) fn insert(&mut self, key: (String, String), node: usize) {
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
pub(super) fn freeze(
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
