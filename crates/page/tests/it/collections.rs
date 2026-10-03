//! Oracle: the default page order (`SortByDefault`) of every page list of the reference builds
//! and of two sites made to stress ties (equal weights, dates and titles, Thai titles), with
//! each site as the current one (`oracle/page/collections/*`). The other list operations
//! (`ByTitle`, `GroupBy`, …) are template functions.

use jiff::Zoned;
use serde_json::Value as J;
use ssg_locale::Collator;
use ssg_page::{SortKey, default_order};

use crate::support::{Tally, family, fixture, idx, s, zoned};

struct Page {
    weight: i32,
    date: Option<Zoned>,
    link_title: String,
    path: String,
    ordinal: Option<u32>,
    weight0: Option<i32>,
}

impl Page {
    fn new(p: &J) -> Self {
        let int = |v: &J| i32::try_from(v.as_i64().expect("int")).expect("i32");
        Self {
            weight: int(&p["weight"]),
            date: zoned(&p["date"]),
            link_title: s(&p["linkTitle"]).to_owned(),
            path: s(&p["pathInfo"]).to_owned(),
            ordinal: p
                .get("ordinal")
                .map(|o| u32::try_from(o.as_u64().expect("ordinal")).expect("u32")),
            weight0: p.get("weight0").map(int),
        }
    }

    fn key(&self) -> SortKey<'_> {
        SortKey {
            weight: self.weight,
            date: self.date.as_ref(),
            link_title: &self.link_title,
            path: &self.path,
            ordinal: self.ordinal,
            weight0: self.weight0,
        }
    }
}

#[test]
fn default_order_matches_go() {
    let mut t = Tally::default();
    for file in family("collections", &["methods.json.gz"]) {
        let fx = fixture(&format!("collections/{file}"));
        let pages: Vec<Page> = fx["pages"]
            .as_array()
            .expect("pages")
            .iter()
            .map(Page::new)
            .collect();
        let collators: Vec<Collator> = fx["sites"]
            .as_array()
            .expect("sites")
            .iter()
            .map(|site| Collator::for_language(s(&site["lang"])))
            .collect();
        let lists = fx["lists"].as_array().expect("lists");
        for c in fx["cases"].as_array().expect("cases") {
            if c["op"] != "SortByDefault" {
                continue;
            }
            let list = &lists[idx(&c["list"])];
            let mut got: Vec<usize> = list["pages"]
                .as_array()
                .expect("list")
                .iter()
                .map(idx)
                .collect();
            let collator = &collators[idx(&c["current"])];
            got.sort_by(|&a, &b| default_order(&pages[a].key(), &pages[b].key(), collator));
            let want: Vec<usize> = c["res"].as_array().expect("res").iter().map(idx).collect();
            t.check(got == want, || {
                format!(
                    "{file} {} (current {}): got {got:?}\n      want {want:?}",
                    list["name"], c["current"]
                )
            });
        }
    }
    t.finish("default-sort");
}
