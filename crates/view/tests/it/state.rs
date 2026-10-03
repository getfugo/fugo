//! The render-state types: pagination recorder (first call wins, identical re-call, conflict
//! naming both positions), page stores (buffered transactions), deferred registry.

use std::path::Path;
use std::sync::Arc;

use ssg_base::diag::Position;
use ssg_base::{FormatId, PageId};
use ssg_nav::{Pagination, PaginationItems};
use ssg_view::{Deferred, DeferredRegistry, PageStores, PaginationRecorder};

fn pages(n: u32) -> PaginationItems {
    PaginationItems::Pages((0..n).map(PageId::from_raw).collect())
}

fn at(line: u32) -> Option<Position> {
    Some(Position {
        file: Arc::from(Path::new("layouts/list.html")),
        line,
        col: 4,
    })
}

#[test]
fn pagination_first_call_wins() {
    let rec = PaginationRecorder::default();
    let (p, f) = (PageId::from_raw(1), FormatId::from_raw(0));
    let first = rec.paginator(p, f, at(1), || {
        Pagination::new(pages(5), 2).expect("pagination")
    });
    let again = rec.paginator(p, f, at(2), || {
        Pagination::new(pages(1), 9).expect("pagination")
    });
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(first.total_pages(), 3);
    assert_eq!(first.page(3), [PageId::from_raw(4)]);
    assert!(first.page(4).is_empty());
    assert_eq!(first.first_call, at(1));

    // paginate with the same list and size reuses the record; another size is an error
    // naming both positions.
    let same = rec
        .paginate(
            p,
            f,
            at(3),
            Pagination::new(pages(5), 2).expect("pagination"),
        )
        .expect("same pagination");
    assert!(Arc::ptr_eq(&first, &same));
    let err = rec
        .paginate(
            p,
            f,
            at(4),
            Pagination::new(pages(5), 3).expect("pagination"),
        )
        .expect_err("conflict");
    assert_eq!((err.first.clone(), err.second.clone()), (at(1), at(4)));
    let msg = err.to_string();
    assert!(
        msg.contains("list.html:1:4") && msg.contains("list.html:4:4"),
        "{msg}"
    );

    let empty = Pagination::new(pages(0), 2).expect("pagination");
    let rec2 = PaginationRecorder::default();
    assert_eq!(rec2.paginator(p, f, None, || empty).total_pages(), 1);
    assert_eq!(rec.recorded().len(), 1);
}

#[test]
fn page_stores_buffer_content_phase_writes() {
    let stores = PageStores::new(3);
    let p = PageId::from_raw(2);
    stores.set(None, p, "direct", tera::Value::from(1));
    assert_eq!(stores.get(None, p, "direct"), Some(tera::Value::from(1)));

    let (won, lost) = (stores.begin(), stores.begin());
    assert_ne!(won, lost);
    stores.set(Some(won), p, "hasMath", tera::Value::from(true));
    stores.set(Some(lost), p, "hasMath", tera::Value::from(false));
    // A transaction reads its own writes; nobody else does before the commit.
    assert_eq!(
        stores.get(Some(won), p, "hasMath"),
        Some(tera::Value::from(true))
    );
    assert_eq!(stores.get(None, p, "hasMath"), None);
    stores.commit(won);
    stores.discard(lost);
    assert_eq!(
        stores.get(None, p, "hasMath"),
        Some(tera::Value::from(true))
    );
    stores.commit(lost); // discarded: nothing left to apply
    assert_eq!(
        stores.get(None, p, "hasMath"),
        Some(tera::Value::from(true))
    );
}

#[test]
fn deferred_first_registration_wins() {
    let reg = DeferredRegistry::default();
    let d = |t: &str| Deferred {
        template: Arc::from(t),
        data: tera::Value::from(t),
    };
    assert_eq!(reg.register("css", d("a.html")), d("a.html"));
    assert_eq!(reg.register("css", d("b.html")), d("a.html"));
    reg.register("js", d("c.html"));
    let keys: Vec<String> = reg.entries().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["css", "js"]);
}
