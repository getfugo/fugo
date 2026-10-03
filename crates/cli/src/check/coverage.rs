//! Lookup coverage: the site model is loaded (as a build does) and every lookup a build makes is
//! run against the layout store: the layout and base template of every (page, format), the
//! template of every shortcode the content calls, and the render hooks of every content page
//! (code block hooks for the fence languages the content uses).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use ssg_base::diag::{Diagnostic, Position};
use ssg_base::{Clock, PageKind};
use ssg_config::Config;
use ssg_layouts::{EmbeddedHooks, HookKind, HookQuery, LayoutStore, ShortcodeQuery};
use ssg_pageparser::{Delim, TokenKind};
use ssg_site::{LoadModelOptions, Page};
use ssg_vfs::Vfs;

use super::display_path;
use crate::args::Coverage;

/// One (page, format) layout lookup.
struct LayoutRow {
    page: String,
    format: String,
    /// `(layout, base)`; `None`: no match.
    chosen: Option<(String, Option<String>)>,
    /// 404, sitemap, sitemap index, robots.txt: no template means no file (not a miss).
    standalone: bool,
}

/// What `templates check` prints after the diagnostics.
pub(crate) struct CoverageReport {
    pages: usize,
    layouts: Vec<LayoutRow>,
    /// (shortcode, template or miss) → the pages calling it.
    shortcodes: BTreeMap<(String, Result<String, String>), BTreeSet<String>>,
    /// (hook kind[:variant], template) → the content pages it applies to (`None`: Markdown's
    /// own rendering).
    hooks: BTreeMap<(String, Option<String>), BTreeSet<String>>,
}

fn is_standalone(kind: PageKind) -> bool {
    matches!(
        kind,
        PageKind::NotFound | PageKind::Sitemap | PageKind::SitemapIndex | PageKind::RobotsTxt
    )
}

fn page_label(cfg: &Config, p: &Page) -> String {
    format!(
        "{} {} {}",
        cfg.sites[p.lang].language.key,
        p.kind.as_str(),
        p.key.to_path()
    )
}

/// The languages of the fenced code blocks of a Markdown body (the first word after ``` or ~~~).
fn fence_languages(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut open: Option<(char, usize)> = None;
    for line in body.lines() {
        let t = line.trim_start();
        if line.len() - t.len() > 3 {
            continue;
        }
        let Some(c) = t.chars().next().filter(|c| matches!(c, '`' | '~')) else {
            continue;
        };
        let n = t.chars().take_while(|x| *x == c).count();
        if n < 3 {
            continue;
        }
        match open {
            Some((oc, on)) if oc == c && n >= on && t[n..].trim().is_empty() => open = None,
            Some(_) => {}
            None => {
                open = Some((c, n));
                let info = t[n..].trim();
                let lang = info
                    .trim_start_matches('{')
                    .split(|x: char| x.is_whitespace() || x == ',' || x == '}')
                    .next()
                    .unwrap_or_default();
                if !lang.is_empty() {
                    out.insert(lang.to_lowercase());
                }
            }
        }
    }
    out
}

/// Runs every lookup; misses become diagnostics. `None` when the site model does not load (a
/// diagnostic says why).
pub(crate) fn run(
    cfg: &Arc<Config>,
    vfs: &Vfs,
    store: &LayoutStore,
    clock: Option<jiff::Timestamp>,
    mode: Coverage,
    diags: &mut Vec<Diagnostic>,
) -> Option<CoverageReport> {
    let root = cfg.project_dir.clone();
    let clock = clock.map_or_else(Clock::system, Clock);
    let model = match ssg_site::load_model(
        Arc::clone(cfg),
        vfs,
        &LoadModelOptions::from_config(cfg, clock),
    ) {
        Ok(m) => m,
        Err(e) => {
            diags.push(
                Diagnostic::error(format!("the site does not load, no lookup coverage: {e}"))
                    .with_id("model"),
            );
            return None;
        }
    };
    let html = cfg.output_formats.by_name("html");
    let mut report = CoverageReport {
        pages: model.pages.len(),
        layouts: Vec::new(),
        shortcodes: BTreeMap::new(),
        hooks: BTreeMap::new(),
    };
    for p in &model.pages {
        let path = ssg_render::lookup_path(p);
        let label = page_label(cfg, p);
        let file = p
            .source
            .as_ref()
            .map(|s| Arc::<Path>::from(display_path(&root, &s.file.abs)));
        for f in ssg_render::rendered_formats(p) {
            let chosen = store
                .select(&ssg_render::layout_query(p, &path, f))
                .map(|s| {
                    (
                        s.layout.to_string(),
                        s.base.as_ref().map(ToString::to_string),
                    )
                });
            let format = cfg.output_formats.get(f).name.clone();
            let standalone = is_standalone(p.kind);
            if chosen.is_none() && !standalone {
                let mut d = Diagnostic::warning(format!(
                    "no layout for {label} in format {format}: the page is not rendered"
                ))
                .with_id("no-layout");
                if let Some(file) = &file {
                    d = d.at(Position {
                        file: Arc::clone(file),
                        line: 0,
                        col: 0,
                    });
                }
                diags.push(d);
            }
            report.layouts.push(LayoutRow {
                page: label.clone(),
                format,
                chosen,
                standalone,
            });
        }
        let (Some(src), Some(html)) = (&p.source, html) else {
            continue;
        };
        let body = src.body();
        // Shortcodes.
        if let Ok(tokens) = ssg_pageparser::lex(body) {
            for w in tokens.windows(2) {
                let (TokenKind::LeftDelim(delim), TokenKind::Name) = (&w[0].kind, &w[1].kind)
                else {
                    continue;
                };
                let name = w[1].text(body);
                let found = store.shortcode(&ShortcodeQuery {
                    name,
                    path: &path,
                    kind: Some(p.kind),
                    lang: Some(p.lang),
                    format: html,
                    markdown: *delim == Delim::Markdown,
                });
                let key = match found {
                    Ok(t) => Ok(t.to_string()),
                    Err(miss) => {
                        let (line, col) =
                            ssg_pageparser::line_col(&src.text, src.body_offset + w[1].span.start);
                        let mut d = Diagnostic::error(format!("{miss} (in {label})"))
                            .with_id("no-shortcode");
                        if let Some(file) = &file {
                            d = d.at(Position {
                                file: Arc::clone(file),
                                line,
                                col,
                            });
                        }
                        diags.push(d);
                        Err(miss.to_string())
                    }
                };
                report
                    .shortcodes
                    .entry((name.to_owned(), key))
                    .or_default()
                    .insert(label.clone());
            }
        }
        // Render hooks.
        let embedded = EmbeddedHooks::of(&cfg.sites[p.lang], cfg);
        let langs = fence_languages(body);
        for kind in HookKind::ALL {
            let variants: Vec<Option<&str>> = if kind == HookKind::CodeBlock {
                std::iter::once(None)
                    .chain(langs.iter().map(|l| Some(l.as_str())))
                    .collect()
            } else {
                vec![None]
            };
            for variant in variants {
                let t = store.hook(&HookQuery {
                    hook: kind,
                    variant,
                    path: &path,
                    kind: Some(PageKind::Page),
                    lang: Some(p.lang),
                    format: html,
                    embedded,
                });
                let name = match variant {
                    Some(v) => format!("{kind}:{v}"),
                    None => kind.to_string(),
                };
                report
                    .hooks
                    .entry((name, t.map(|t| t.to_string())))
                    .or_default()
                    .insert(label.clone());
            }
        }
    }
    if mode == Coverage::Summary {
        // Code block languages without a hook of their own add nothing to the summary.
        report
            .hooks
            .retain(|(name, t), _| match name.split_once(':') {
                None => true,
                Some((kind, v)) => t
                    .as_deref()
                    .is_some_and(|t| t.contains(&format!("render-{kind}-{v}"))),
            });
    }
    Some(report)
}

impl CoverageReport {
    pub(crate) fn write(&self, out: &mut dyn Write, mode: Coverage) -> io::Result<()> {
        writeln!(
            out,
            "\nlayout lookups: {} pages, {} (page, format) queries",
            self.pages,
            self.layouts.len()
        )?;
        let chosen = |r: &LayoutRow| match &r.chosen {
            Some((l, Some(b))) => format!("{l} + {b}"),
            Some((l, None)) => l.clone(),
            None if r.standalone => "(no template: not written)".to_owned(),
            None => "NO MATCH".to_owned(),
        };
        if mode == Coverage::Full {
            for r in &self.layouts {
                writeln!(out, "  {} [{}] → {}", r.page, r.format, chosen(r))?;
            }
        } else {
            let mut groups: BTreeMap<String, usize> = BTreeMap::new();
            for r in self.layouts.iter().filter(|r| r.chosen.is_some()) {
                *groups.entry(chosen(r)).or_default() += 1;
            }
            for (t, n) in &groups {
                writeln!(out, "  {t}: {n}")?;
            }
            for r in self.layouts.iter().filter(|r| r.chosen.is_none()) {
                writeln!(out, "  {} [{}] → {}", r.page, r.format, chosen(r))?;
            }
        }
        writeln!(out, "\nshortcodes ({} names):", {
            self.shortcodes
                .keys()
                .map(|(n, _)| n)
                .collect::<BTreeSet<_>>()
                .len()
        })?;
        for ((name, t), pages) in &self.shortcodes {
            let t = match t {
                Ok(t) => t.clone(),
                Err(miss) => format!("NO MATCH ({miss})"),
            };
            writeln!(out, "  {name} → {t}: {} page(s)", pages.len())?;
            if mode == Coverage::Full || t.starts_with("NO MATCH") {
                for p in pages {
                    writeln!(out, "      {p}")?;
                }
            }
        }
        writeln!(out, "\nrender hooks (content pages, HTML):")?;
        for ((name, t), pages) in &self.hooks {
            let t = t.as_deref().unwrap_or("(Markdown rendering)");
            writeln!(out, "  {name} → {t}: {} page(s)", pages.len())?;
            if mode == Coverage::Full {
                for p in pages {
                    writeln!(out, "      {p}")?;
                }
            }
        }
        writeln!(out)
    }
}
