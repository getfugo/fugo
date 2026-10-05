//! The generated sites of `cargo dev bench`: Markdown pages in ten sections, with tags and
//! categories, headings (and a table of contents), a list, a table, a highlighted code block and
//! links, listed on paginated section, taxonomy and term pages, with RSS and a sitemap. The text
//! is the same for every run (a fixed word list and a linear congruential generator), so two
//! commits build the same site.

use std::fmt::Write as _;
use std::path::Path;

use super::Templates;
use crate::{Fail, fail};

const SECTIONS: usize = 10;
const TAGS: usize = 40;
const CATEGORIES: usize = 6;

const WORDS: &[&str] = &[
    "almond", "biscuit", "caramel", "cracker", "crisp", "crunchy", "dried", "flavour", "fried",
    "ginger", "honey", "jelly", "lemon", "mango", "nut", "orange", "pepper", "pretzel", "rice",
    "roasted", "salted", "seaweed", "sesame", "smoky", "snack", "spicy", "sweet", "tamarind",
    "toasted", "wafer", "the", "a", "with", "and", "for", "every", "bag", "box", "pack", "taste",
];

const CONFIG: &str = r#"baseURL = "https://bench.example/"
title = "Generated benchmark site"
languageCode = "en-US"
timeZone = "UTC"

[pagination]
pagerSize = 20

[markup.tableOfContents]
startLevel = 2
endLevel = 3
"#;

/// A linear congruential generator (Knuth's MMIX constants): the same text on every machine.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The high bits are the random ones; `n` is small.
        usize::try_from(self.0 >> 33).unwrap_or(0) % n
    }

    fn words(&mut self, n: usize) -> String {
        let words: Vec<&str> = (0..n).map(|_| WORDS[self.next(WORDS.len())]).collect();
        words.join(" ")
    }

    fn sentence(&mut self) -> String {
        let n = 8 + self.next(10);
        let mut s = self.words(n);
        if let Some(first) = s.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        s.push('.');
        s
    }

    fn paragraph(&mut self) -> String {
        let n = 3 + self.next(4);
        (0..n)
            .map(|_| self.sentence())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Writes a site of `pages` pages with `templates` into the directory `dir`, which must not exist.
pub fn generate(dir: &Path, pages: usize, templates: Templates) -> Result<(), Fail> {
    if dir.exists() {
        return Err(fail!("{}: exists", dir.display()));
    }
    let write = |rel: &str, text: &str| -> Result<(), Fail> {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| fail!("{}: {e}", path.display()))
    };

    let mut config = CONFIG.to_owned();
    for s in 0..SECTIONS {
        let _ = write!(
            config,
            "\n[[menus.main]]\nname = \"Section {s}\"\npageRef = \"/section-{s}/\"\nweight = {}\n",
            s + 1
        );
    }
    write("config.toml", &config)?;
    for (name, text) in templates.layouts() {
        write(&format!("layouts/{name}"), text)?;
    }

    let mut rng = Lcg(0x5eed);
    write(
        "content/_index.md",
        &format!("---\ntitle: Home\n---\n\n{}\n", rng.paragraph()),
    )?;
    for s in 0..SECTIONS {
        write(
            &format!("content/section-{s}/_index.md"),
            &format!("---\ntitle: Section {s}\n---\n\n{}\n", rng.paragraph()),
        )?;
    }
    for n in 0..pages {
        write(
            &format!("content/section-{}/page-{n}.md", n % SECTIONS),
            &page(&mut rng, n),
        )?;
    }
    Ok(())
}

/// The Markdown file of page `n`.
fn page(rng: &mut Lcg, n: usize) -> String {
    let n_words = 3 + rng.next(3);
    let title = rng.words(n_words);
    let tags: Vec<String> = (0..3)
        .map(|_| format!("\"tag-{}\"", rng.next(TAGS)))
        .collect();
    let category = rng.next(CATEGORIES);
    // A page an hour, from 2020-01-01 on.
    let (day, hour) = (n / 24, n % 24);
    let date = jiff::civil::date(2020, 1, 1)
        .checked_add(jiff::Span::new().days(i64::try_from(day).unwrap_or(0)))
        .unwrap_or(jiff::civil::date(2020, 1, 1));
    let mut md = format!(
        "---\ntitle: \"Page {n}: {title}\"\ndate: {date}T{hour:02}:00:00Z\ntags: [{}]\ncategories: [\"category-{category}\"]\n---\n\n{}\n\n<!--more-->\n",
        tags.join(", "),
        rng.paragraph(),
    );
    for h in 0..3 {
        let _ = write!(md, "\n## {}\n\n{}\n", rng.words(3), rng.paragraph());
        if h == 0 {
            let _ = write!(md, "\n### {}\n\n", rng.words(2));
            for _ in 0..4 {
                let _ = writeln!(md, "- **{}** {}", rng.words(1), rng.words(6));
            }
        }
    }
    md.push_str("\n| Snack | Flavour | Grams |\n|---|---|---:|\n");
    for _ in 0..4 {
        let _ = writeln!(
            md,
            "| {} | {} | {} |",
            rng.words(2),
            rng.words(1),
            10 + rng.next(490)
        );
    }
    let _ = write!(
        md,
        "\n```rust\nfn snack(name: &str) -> usize {{\n    // {}\n    name.len() * {}\n}}\n```\n",
        rng.words(5),
        1 + rng.next(9),
    );
    if n > 0 {
        let _ = write!(
            md,
            "\nSee also [the page before](/section-{}/page-{}/) and *{}*.\n",
            (n - 1) % SECTIONS,
            n - 1,
            rng.words(2),
        );
    }
    md
}
