//! Unit cases the oracles do not cover: the default pagination list per kind, menu parent
//! cycles, and group pagination arithmetic.

use std::sync::Arc;

use ssg_base::url::{Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};
use ssg_base::{Idx, LangIdx, PageId, PageKind, Params, Value};
use ssg_config::sections::MenuEntryConfig;
use ssg_nav::{
    MenuOptions, NavModel, PageFacts, PageGroup, PagerSlice, Pagination, PaginationItems,
    Rendering, build_site_menus, default_pagination_list,
};
use ssg_page::{Dates, ListMode};

/// Five pages: home, section, taxonomy, term, 404; each list is distinct.
struct Toy {
    kinds: Vec<PageKind>,
    lists: Vec<Vec<PageId>>,
    dates: Dates,
    params: Params,
}

fn id(i: usize) -> PageId {
    PageId::from_index(i)
}

impl Toy {
    fn new() -> Self {
        Self {
            kinds: vec![
                PageKind::Home,
                PageKind::Section,
                PageKind::Taxonomy,
                PageKind::Term,
                PageKind::NotFound,
            ],
            lists: vec![vec![id(1)], vec![id(2)], vec![id(3)]],
            dates: Dates::default(),
            params: Params::default(),
        }
    }
}

impl NavModel for Toy {
    fn page(&self, page: PageId) -> PageFacts<'_> {
        PageFacts {
            id: page,
            lang: LangIdx::from_index(0),
            kind: self.kinds[page.index()],
            lang_key: "en",
            section: "",
            title: "",
            link_title: "",
            name: "",
            slug: "",
            description: "",
            page_type: "",
            layout: "",
            bundle_type: "",
            draft: false,
            weight: 0,
            keywords: &[],
            aliases: &[],
            dates: &self.dates,
            params: &self.params,
            menus: &[],
            rel_permalink: "",
            fragments: &[],
            outputs: &[],
            rendering: Rendering::Rendered,
            list: ListMode::Always,
        }
    }
    fn tree_pages(&self, _: LangIdx) -> Vec<PageId> {
        Vec::new()
    }
    fn resolve_page_ref(&self, _: LangIdx, _: &str) -> Option<PageId> {
        None
    }
    fn is_ancestor(&self, _: PageId, _: PageId) -> bool {
        false
    }
    fn pages(&self, _: PageId) -> &[PageId] {
        &self.lists[0]
    }
    fn regular_pages(&self, _: PageId) -> &[PageId] {
        &self.lists[1]
    }
    fn site_regular_pages(&self, _: LangIdx) -> &[PageId] {
        &self.lists[2]
    }
    fn home(&self, _: LangIdx) -> Option<PageId> {
        Some(id(0))
    }
}

#[test]
fn default_pagination_list_by_kind() {
    let m = Toy::new();
    let list = |i| default_pagination_list(&m, id(i)).to_vec();
    assert_eq!(list(0), vec![id(3)], "home: site regular pages");
    assert_eq!(list(1), vec![id(2)], "section: regular pages");
    assert_eq!(list(2), vec![id(1)], "taxonomy: pages");
    assert_eq!(list(3), vec![id(1)], "term: pages");
    assert_eq!(list(4), vec![id(3)], "404: site regular pages");
}

#[test]
fn menu_parent_cycle_is_cut() {
    let entry = |name: &str, parent: &str| MenuEntryConfig {
        menu: "main".into(),
        identifier: name.into(),
        name: name.into(),
        parent: parent.into(),
        ..MenuEntryConfig::default()
    };
    let entries = [entry("a", "b"), entry("b", "a"), entry("c", "")];
    let urls = SiteUrls {
        base_url: BaseUrl::parse("https://example.org/").expect("url"),
        language_prefix: String::new(),
        link_style: LinkStyle::Relative,
        path_case: PathCase::Lower,
        accents: Accents::Keep,
    };
    let opts = MenuOptions {
        entries: &entries,
        section_pages_menu: None,
        urls,
    };
    let (menus, _) = build_site_menus(&Toy::new(), LangIdx::from_index(0), &opts);
    // `a` and `b` are each other's parent: neither is top level, and building terminates.
    let top: Vec<&str> = menus.menu("main").iter().map(|e| e.name.as_str()).collect();
    assert_eq!(top, ["c"]);
}

#[test]
fn group_pagination_spans_groups() {
    let g = |key: &str, n: usize, from: usize| PageGroup {
        key: Value::string(key),
        pages: (from..from + n).map(id).collect(),
    };
    let items = PaginationItems::Groups(Arc::from([g("a", 3, 0), g("b", 2, 3)]));
    let p = Pagination::new(items, 2).expect("size");
    assert_eq!(p.total_pages(), 3);
    let slices: Vec<Vec<(usize, std::ops::Range<usize>)>> = p
        .pagers()
        .into_iter()
        .map(|pg| match pg.slice {
            PagerSlice::Groups(g) => g.into_iter().map(|s| (s.group, s.range)).collect(),
            PagerSlice::Pages(_) => unreachable!(),
        })
        .collect();
    assert_eq!(
        slices,
        vec![vec![(0, 0..2)], vec![(0, 2..3), (1, 0..1)], vec![(1, 1..2)]]
    );
    assert!(Pagination::new(PaginationItems::Pages(Arc::from([])), 0).is_err());
    let empty = Pagination::new(PaginationItems::Pages(Arc::from([])), 3).expect("size");
    assert_eq!((empty.total_pages(), empty.pager_count()), (0, 1));
}
