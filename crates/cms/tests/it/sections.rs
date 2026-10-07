//! The sections of the index: their pages, their folders (the terms of a taxonomy), and how new
//! pages are written in a section that has only folders.

use ssg_cms::index::Section;

use crate::fields::read;

/// The sections of a site with snacks in a folder of their own, the term pages of brands (in
/// TOML, with language suffixes), and those of tags, whose section has no index page.
fn sections() -> Vec<Section> {
    read(
        "",
        &[
            ("content/_index.md", "---\ntitle: Home\n---\n"),
            ("content/snacks/_index.md", "---\ntitle: Snacks\n---\n"),
            ("content/snacks/chips/_index.md", "---\ntitle: Chips\n---\n"),
            (
                "content/snacks/chips/lays/index.md",
                "---\ntitle: Lay's\n---\n",
            ),
            ("content/snacks/wafers.md", "---\ntitle: Wafers\n---\n"),
            (
                "content/brands/_index.en.md",
                "+++\ntitle = \"Brands\"\n+++\n",
            ),
            (
                "content/brands/lays/_index.en.md",
                "+++\ntitle = \"Lay's\"\ndescription = \"Chips\"\n+++\n",
            ),
            (
                "content/brands/lays/_index.th.md",
                "+++\ntitle = \"เลย์\"\n+++\n",
            ),
            (
                "content/brands/pocky/_index.en.md",
                "+++\ntitle = \"Pocky\"\n+++\n",
            ),
            ("content/tags/salty/_index.md", "---\ntitle: Salty\n---\n"),
        ],
    )
    .sections
}

fn section<'a>(sections: &'a [Section], key: &str) -> &'a Section {
    sections
        .iter()
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("no section {key:?}"))
}

#[test]
fn a_section_counts_its_pages_and_its_folders_apart() {
    let sections = sections();
    let snacks = section(&sections, "snacks");
    assert_eq!(
        (snacks.count, snacks.folders),
        (2, 1),
        "its own index page is neither"
    );
    let brands = section(&sections, "brands");
    assert_eq!(brands.title, "Brands");
    assert_eq!((brands.count, brands.folders), (0, 2));
    let home = section(&sections, "");
    assert_eq!((home.count, home.folders), (0, 0));
}

#[test]
fn a_section_of_folders_without_an_index_page_is_listed() {
    let sections = sections();
    let tags = section(&sections, "tags");
    assert_eq!(tags.title, "Tags");
    assert_eq!((tags.count, tags.folders), (0, 1));
}

#[test]
fn new_pages_of_a_section_without_pages_are_written_like_its_folders() {
    let sections = sections();
    let brands = section(&sections, "brands");
    assert_eq!(brands.style.format, "toml");
    assert!(brands.style.lang_suffix, "{:?}", brands.style);
    assert!(!brands.style.bundle);
    let keys: Vec<&str> = brands.keys.iter().map(|k| k.key.as_str()).collect();
    assert!(keys.contains(&"description"), "{keys:?}");
    // A section with pages takes them only.
    let snacks = section(&sections, "snacks");
    assert_eq!(snacks.style.format, "yaml");
    assert!(!snacks.style.lang_suffix);
}
