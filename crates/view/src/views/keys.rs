//! The documented keys of each view (what templates may read), checked against the views by the
//! tests.

/// The keys of a summary page value (every generation).
pub const PAGE_SUMMARY_KEYS: &[&str] = &[
    "id",
    "kind",
    "lang",
    "path",
    "section",
    "type",
    "layout",
    "bundle_type",
    "name",
    "title",
    "link_title",
    "description",
    "date",
    "lastmod",
    "publish_date",
    "expiry_date",
    "weight",
    "draft",
    "params",
    "keywords",
    "aliases",
    "permalink",
    "rel_permalink",
    "is_home",
    "is_section",
    "is_page",
    "is_node",
    "is_translated",
    "file",
    "git_info",
    "sitemap",
    "language",
    "output_formats",
    "resources",
    "terms",
    "raw_content",
];

/// The content keys a page value has in the Full generations only.
pub const CONTENT_KEYS: &[&str] = &[
    "content",
    "summary",
    "truncated",
    "plain",
    "word_count",
    "fuzzy_word_count",
    "reading_time",
    "table_of_contents",
    "fragments",
    "len",
];

/// The keys a full page value adds to its summary (the rendered page, `deref`, `get_page`).
pub const PAGE_RELATION_KEYS: &[&str] = &[
    "parent",
    "current_section",
    "first_section",
    "ancestors",
    "pages",
    "regular_pages",
    "regular_pages_recursive",
    "sections",
    "prev",
    "next",
    "prev_in_section",
    "next_in_section",
    "translations",
    "all_translations",
    "alternative_output_formats",
    "taxonomy",
    "term",
];

/// The keys of `site`.
pub const SITE_KEYS: &[&str] = &[
    "title",
    "base_url",
    "lang",
    "language_code",
    "language",
    "languages",
    "is_multilingual",
    "copyright",
    "params",
    "data",
    "home",
    "pages",
    "regular_pages",
    "all_pages",
    "sections",
    "main_sections",
    "taxonomies",
    "menus",
    "last_mod",
    "config",
    "sitemap_abs_url",
    "server_port",
];

/// The keys of a page link (`page.terms.<plural>[i]`, alias pages, menu entries' `page`).
pub const PAGE_LINK_KEYS: &[&str] = &[
    "id",
    "kind",
    "path",
    "lang",
    "title",
    "link_title",
    "permalink",
    "rel_permalink",
];

/// The keys of a resource value.
pub const RESOURCE_KEYS: &[&str] = &[
    "__rid",
    "name",
    "title",
    "params",
    "resource_type",
    "media_type",
    "rel_permalink",
    "permalink",
    "width",
    "height",
    "data",
    "page_id",
];

/// The keys of a menu entry.
pub const MENU_ENTRY_KEYS: &[&str] = &[
    "identifier",
    "key_name",
    "name",
    "title",
    "url",
    "weight",
    "parent",
    "pre",
    "post",
    "params",
    "page",
    "children",
    "has_children",
];

/// The keys of a pager (`paginator()`, `paginate()`).
pub const PAGER_KEYS: &[&str] = &[
    "page_number",
    "url",
    "pages",
    "pager_size",
    "total_pages",
    "total_number_of_elements",
    "has_prev",
    "has_next",
    "prev",
    "next",
    "first",
    "last",
    "pagers",
];
