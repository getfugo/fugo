//! Real sites (`tools/rust-port/i01/sites.py make <site> <dir>`): the Meta generation and two
//! Full generations (`Html` and a format variant) for every page, every documented key, list
//! sharing, and the heap the views keep compared with the model's (dhat).
//!
//! `FUGO_SITES=<dir>[:<dir>…] cargo test -p ssg-view real_sites -- --ignored
//! --nocapture` (alone: the heap is measured process-wide). A directory named `docs` must keep
//! its views below twice the model's heap.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use ssg_base::{IdVec, PageId};
use ssg_config::site::EmojiPolicy;
use ssg_markup::{ExpandedMarkdown, MarkdownOptions, NoHooks, SourceContexts, text};
use ssg_site::Model;
use ssg_view::views::{CONTENT_KEYS, PAGE_RELATION_KEYS, PAGE_SUMMARY_KEYS};
use ssg_view::{Contents, HookVariant, Phase, RenderedContent};

use crate::support::{get, has_content, keys, load_model, same, views_of};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn heap() -> (u64, u64) {
    let s = dhat::HeapStats::get();
    (s.curr_bytes as u64, s.total_bytes)
}

/// The HTML variant's content of every page: Markdown without hooks (shortcodes stay text).
fn render_contents(m: &Model) -> Contents {
    m.pages
        .iter()
        .map(|p| {
            if !has_content(m, p.id) {
                return None;
            }
            let src = p.source.as_ref()?;
            let site = &m.config.sites[p.lang];
            let o = MarkdownOptions::from_config(&site.markup, site.emoji == EmojiPolicy::Enabled);
            let file: Arc<Path> = Arc::from(src.file.abs.as_path());
            let contexts = SourceContexts::default();
            let r = ssg_markup::render(
                &ExpandedMarkdown {
                    text: src.body(),
                    page: p.id,
                    contexts: &contexts,
                    file: &file,
                },
                &o,
                &NoHooks,
                None,
            )
            .ok()?;
            let summary = text::auto_summary(&r.html, site.summary_length, false);
            let plain = text::strip_html(&r.html);
            let words = text::word_count(&plain, false);
            Some(Arc::new(RenderedContent {
                table_of_contents: r.toc.to_html(&o.toc),
                summary: summary.html,
                truncated: summary.truncated,
                html: r.html,
                plain,
                word_count: words,
                fuzzy_word_count: words.div_ceil(100) * 100,
                reading_time: words.div_ceil(213),
                fragments: Arc::new(r.fragments),
            }))
        })
        .collect::<IdVec<PageId, _>>()
}

fn check_site(dir: &Path) {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (h0, t0) = heap();
    let model = Arc::new(load_model(dir));
    let (h1, t1) = heap();
    let contents = render_contents(&model);
    let variant = model
        .config
        .output_formats
        .iter()
        .find(|(_, f)| !f.is_html)
        .map(|(id, _)| HookVariant::Format(id))
        .expect("a non-HTML format");
    let model_bytes = h1 - h0;
    let text: usize = model
        .pages
        .iter()
        .filter_map(|p| p.source.as_ref())
        .map(|s| s.text.len())
        .sum();
    let body: usize = model
        .pages
        .iter()
        .filter_map(|p| p.source.as_ref())
        .map(|s| s.body().len())
        .sum();
    let html: usize = contents
        .iter()
        .flatten()
        .map(|c| c.html.len() + c.summary.len() + c.plain.len() + c.table_of_contents.len())
        .sum();
    println!("{name}: source text {text} B (bodies {body} B); content strings {html} B");
    #[allow(clippy::cast_precision_loss)]
    let ratio = |b: u64| b as f64 / model_bytes as f64;
    println!(
        "{name}: {} pages, {} languages; model {model_bytes} B kept ({} B allocated)",
        model.pages.len(),
        model.sites.len(),
        t1 - t0,
    );

    // As a build with one hook variant keeps them: the Meta generation and the Full
    // generation. Every layout job renders its page's full value (`page_value`, built for the
    // job and dropped with it).
    let (h2, t2) = heap();
    let (store, views) = views_of(&model);
    let (hm, _) = heap();
    let mut html = BTreeMap::new();
    html.insert(HookVariant::Html, contents.clone());
    views.freeze(&html);
    let (hf, _) = heap();
    let g = views.generation(Phase::Layout, HookVariant::Html);
    for id in model.pages.ids() {
        drop(g.page_value(id));
    }
    let (h3, t3) = heap();
    let core = h3 - h2;
    println!(
        "{name}: views (Meta {} B + Full summaries and sites {} B + job values {} B) {core} B \
         kept ({} B allocated): {:.2}× the model",
        hm - h2,
        hf - hm,
        h3 - hf,
        t3 - t2,
        ratio(core)
    );
    for id in model.pages.ids() {
        let _ = g.full(id);
    }
    let cached = heap().0 - h2;
    println!(
        "{name}: with every full value cached as well: {cached} B: {:.2}× the model",
        ratio(cached)
    );
    drop((store, views));

    // The worst case: two Full generations and every full value of all three generations.
    let (h4, _) = heap();
    let (_store, views) = views_of(&model);
    let mut all = BTreeMap::new();
    all.insert(HookVariant::Html, contents.clone());
    all.insert(variant, contents);
    views.freeze(&all);
    let generations = [
        views.meta(),
        views.generation(Phase::Layout, HookVariant::Html),
        views.generation(Phase::Layout, variant),
    ];
    for g in generations {
        for id in model.pages.ids() {
            let _ = g.full(id);
        }
    }
    let worst = heap().0 - h4;
    println!(
        "{name}: worst case (Meta + 2 Full, all full values forced) {worst} B kept: {:.2}× the \
         model",
        ratio(worst)
    );

    let mut full_keys: Vec<String> = [PAGE_SUMMARY_KEYS, CONTENT_KEYS, PAGE_RELATION_KEYS]
        .concat()
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
    full_keys.sort();
    let mut meta_keys: Vec<String> = [PAGE_SUMMARY_KEYS, PAGE_RELATION_KEYS]
        .concat()
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
    meta_keys.sort();
    let mut shared = 0usize;
    for (i, g) in generations.into_iter().enumerate() {
        for id in model.pages.ids() {
            let full = g.full(id);
            let want = if i == 0 { &meta_keys } else { &full_keys };
            assert_eq!(&keys(&full), want, "{name} {id}");
            for v in get(&full, "pages").as_array().expect("pages") {
                let q = PageId::from_raw(
                    u32::try_from(get(v, "id").as_u64().expect("id")).expect("u32"),
                );
                assert!(same(v, &g.summaries[q]));
                shared += 1;
            }
        }
    }
    println!("{name}: every full value has every documented key; {shared} list entries shared");
    if name == "docs" {
        assert!(
            ratio(core) < 2.0,
            "docs views keep {:.2}× the model's heap",
            ratio(core)
        );
    }
}

#[test]
#[ignore = "needs sites written by sites.py (FUGO_SITES)"]
fn real_sites() {
    let dirs = std::env::var("FUGO_SITES").expect("FUGO_SITES");
    let _profiler = dhat::Profiler::builder().testing().build();
    for d in dirs.split(':').filter(|d| !d.is_empty()) {
        check_site(Path::new(d));
    }
}
