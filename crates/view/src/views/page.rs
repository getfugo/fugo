//! The views of pages: summaries, relations, taxonomies and terms, and page links.

use super::*;

/// The relation-free page value, Arc-shared in every list. The Full generations insert the
/// [`ContentView`] keys into it. Values that many pages share (dates, sitemap settings, media
/// types, empty lists) are pre-serialised once and passed through.
#[derive(Clone, Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)] // template fields (`is_home`, `draft`, …), not state
pub struct PageSummaryView {
    pub id: u32,
    pub kind: PageKind,
    pub lang: String,
    pub path: String,
    pub section: String,
    pub r#type: String,
    pub layout: Option<String>,
    /// `leaf`, `branch`, or none.
    pub bundle_type: Option<&'static str>,
    /// `.Name`.
    pub name: String,
    pub title: String,
    pub link_title: String,
    pub description: String,
    /// `DateView` or none (as the three below).
    pub date: tera::Value,
    pub lastmod: tera::Value,
    pub publish_date: tera::Value,
    pub expiry_date: tera::Value,
    pub weight: i32,
    pub draft: bool,
    pub params: tera::Value,
    /// `[String]` (as `aliases`).
    pub keywords: tera::Value,
    pub aliases: tera::Value,
    /// The primary format's links (`""` when the page has no link).
    pub permalink: String,
    pub rel_permalink: String,
    pub is_home: bool,
    pub is_section: bool,
    pub is_page: bool,
    pub is_node: bool,
    pub is_translated: bool,
    pub file: Option<FileView>,
    pub git_info: Option<GitInfoView>,
    /// `SitemapView`.
    pub sitemap: tera::Value,
    /// `LanguageView`.
    pub language: tera::Value,
    /// `{name: OutputFormatView}` in format order.
    pub output_formats: tera::Value,
    /// `[ResourceView]`.
    pub resources: tera::Value,
    /// `{plural: [PageLink]}`, a key for every configured taxonomy.
    pub terms: tera::Value,
    /// `.RawContent`: the source after the front matter. Known after parsing, so it is in
    /// every generation (shortcodes and hooks read other pages' sources through it); one
    /// value per page, shared by the generations.
    pub raw_content: tera::Value,
}

/// The relations a full page value adds on top of its summary. Every list holds summary values
/// of the same generation (acyclic, Arc-shared).
#[derive(Clone, Debug, Serialize)]
pub struct PageRelations {
    pub parent: Option<tera::Value>,
    pub current_section: tera::Value,
    pub first_section: tera::Value,
    pub ancestors: tera::Value,
    pub pages: tera::Value,
    pub regular_pages: tera::Value,
    pub regular_pages_recursive: tera::Value,
    pub sections: tera::Value,
    pub prev: Option<tera::Value>,
    pub next: Option<tera::Value>,
    pub prev_in_section: Option<tera::Value>,
    pub next_in_section: Option<tera::Value>,
    pub translations: tera::Value,
    pub all_translations: tera::Value,
    /// The output formats other than the primary one.
    pub alternative_output_formats: tera::Value,
    /// Taxonomy pages.
    pub taxonomy: Option<TaxonomyView>,
    /// Term pages.
    pub term: Option<TermView>,
}

/// `page.taxonomy` of a taxonomy page (`.Data.Singular/Plural/Terms`).
#[derive(Clone, Debug, Serialize)]
pub struct TaxonomyView {
    pub singular: String,
    pub plural: String,
    /// `[TermEntryView]` by term key.
    pub terms: tera::Value,
}

/// `page.term` of a term page.
#[derive(Clone, Debug, Serialize)]
pub struct TermView {
    /// `.Name`: the term as first written.
    pub name: String,
    /// `.Data.Term`.
    pub term: String,
    /// The term's key (`blue-sky`).
    pub key: String,
    pub singular: String,
    pub plural: String,
}

/// A term with its pages (`site.taxonomies.<plural>.<key>`, `page.taxonomy.terms`).
#[derive(Clone, Debug, Serialize)]
pub struct TermEntryView {
    pub name: String,
    pub key: String,
    pub count: usize,
    /// The term page (summary).
    pub page: tera::Value,
    /// Its members (summaries): weight, then the default order.
    pub pages: tera::Value,
}

/// A reference to a page (terms, alias pages, menu entries).
#[derive(Clone, Debug, Serialize)]
pub struct PageLink {
    pub id: u32,
    pub kind: PageKind,
    pub path: String,
    pub lang: String,
    pub title: String,
    pub link_title: String,
    pub permalink: String,
    pub rel_permalink: String,
}
