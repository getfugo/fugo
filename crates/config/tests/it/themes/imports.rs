//! Finding themes: `[[module.imports]]`, replacements, theme lists and merge strategies.

use super::*;

/// `TestLoadConfigModules`: the module graph of `[[module.imports]]`, old-style `theme` lists
/// in a theme's `config.toml`, and `theme.toml` metadata (not configuration).
#[test]
fn load_config_modules() {
    let mut files = vec![(
        "config.toml",
        "[module]\n[[module.imports]]\npath=\"n1\"\n[[module.imports]]\npath=\"n4\"\n",
    )];
    files.extend([
        (
            "themes/n1/config.toml",
            "title = \"Component n1\"\n\n[module]\ndescription = \"Component n1 description\"\n[[module.imports]]\npath=\"o1\"\n[[module.imports]]\npath=\"n3\"\n",
        ),
        ("themes/n2/config.toml", "title = \"Component n2\"\n"),
        ("themes/n3/config.toml", "title = \"Component n3\"\n"),
        ("themes/n4/config.toml", "title = \"Component n4\"\n"),
        ("themes/o1/config.toml", "theme = [\"n2\"]\n"),
        (
            "themes/o1/theme.toml",
            "name = \"Component o1\"\nlicense = \"MIT\"\nmin_version = 0.38\n",
        ),
    ]);
    let p = Project::new(&files);
    for n in ["n1", "n2", "n3", "n4", "o1"] {
        let data = p.dir().join(format!("themes/{n}/data"));
        std::fs::create_dir_all(&data).expect("dir");
        std::fs::write(data.join("module.toml"), format!("name={n:?}")).expect("write");
    }
    let c = p.ok();
    let graph: Vec<String> = c
        .themes
        .iter()
        .map(|t| format!("{} {}", t.owner.as_deref().unwrap_or("project"), t.path))
        .collect();
    assert_eq!(
        graph,
        ["project n1", "n1 o1", "o1 n2", "n1 n3", "project n4"]
    );
    // Root values of themes are not merged by default.
    assert_eq!(c.default_site().title, "");
}

/// `modules/config_test.go` `TestDecodeConfigBothOldAndNewProvided` and
/// `TestDecodeConfigTheme`: `[[module.imports]]` first, then `theme`.
#[test]
fn imports_then_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"b\", \"c\"]\n\n[module]\n[[module.imports]]\npath=\"a\"\n",
        ),
        ("themes/a/layouts/a.html", ""),
        ("themes/b/layouts/b.html", ""),
        ("themes/c/layouts/c.html", ""),
    ]);
    let paths: Vec<String> = p.ok().themes.into_iter().map(|t| t.path).collect();
    assert_eq!(paths, ["a", "b", "c"]);
    let p = Project::new(&[
        ("config.toml", "theme = [\"a\", \"b\"]\n"),
        ("themes/a/layouts/a.html", ""),
        ("themes/b/layouts/b.html", ""),
    ]);
    let paths: Vec<String> = p.ok().themes.into_iter().map(|t| t.path).collect();
    assert_eq!(paths, ["a", "b"]);
}

/// `TestDecodeConfig` "Replacements": a comma-separated string or a list; the replaced path is
/// read from the themes directory.
#[test]
fn module_replacements() {
    for replacements in [
        "replacements=\"a->b,github.com/bep/mycomponent->c\"",
        "replacements=[\"a->b\",\"github.com/bep/mycomponent->c\"]",
    ] {
        let p = Project::new(&[
            (
                "config.toml",
                &format!(
                    "[module]\n{replacements}\n[[module.imports]]\npath=\"github.com/bep/mycomponent\"\n"
                ),
            ),
            ("themes/c/config.toml", "[params]\nfromC = true\n"),
        ]);
        let c = p.ok();
        assert_eq!(c.themes.len(), 1, "{replacements}");
        assert_eq!(c.themes[0].path, "c");
        assert_eq!(c.themes[0].dir, p.dir().join("themes/c"));
        assert_eq!(params(&c), toml("fromc = true\n"));
    }
    // Not "old -> new".
    let p = Project::new(&[(
        "config.toml",
        "[module]\nreplacements = [\"github.com/a/b\"]\n",
    )]);
    let e = p.load().expect_err("invalid replacement");
    assert!(e.to_string().contains("module.replacements"), "{e}");
}

/// `modules/collect_test.go` `TestPathKey`.
#[test]
fn import_path_identity() {
    for (path, key) in [
        ("github.com/foo", "github.com/foo"),
        ("github.com/foo/v2", "github.com/foo"),
        ("github.com/foo/v12", "github.com/foo"),
        ("github.com/foo/v3d", "github.com/foo/v3d"),
        ("MyTheme", "mytheme"),
    ] {
        assert_eq!(path_key(path), key, "{path}");
    }
}

#[test]
fn theme_list_precedence() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"a\", \"b\"]\n[params]\nmine = 1\n",
        ),
        (
            "themes/a/config.toml",
            "[params]\nx = \"a\"\n[params.deep]\nfromA = 1\n",
        ),
        (
            "themes/b/config.toml",
            "[params]\nx = \"b\"\ny = \"b\"\n[params.deep]\nfromA = 2\nfromB = 2\n",
        ),
    ]);
    assert_eq!(
        params(&p.ok()),
        toml("mine = 1\nx = \"a\"\ny = \"b\"\n[deep]\nfroma = 1\nfromb = 2\n")
    );
}

/// A theme's `_merge` applies to the tables it brings in when a later theme is merged into
/// them; on a table the project has, the project's strategy decides.
#[test]
fn merge_strategy_written_in_a_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"a\", \"b\"]\n[params.own]\nkept = 1\n",
        ),
        (
            "themes/a/config.toml",
            "[params.locked]\n_merge = \"none\"\nfromA = 1\n[params.own]\n_merge = \"none\"\nfromA = 1\n",
        ),
        (
            "themes/b/config.toml",
            "[params.locked]\nfromB = 2\n[params.own]\nfromB = 2\n",
        ),
    ]);
    assert_eq!(
        params(&p.ok()),
        toml("[locked]\nfroma = 1\n[own]\nkept = 1\nfroma = 1\nfromb = 2\n")
    );
}

#[test]
fn per_key_defaults_and_overrides() {
    let theme = r#"
title = "theme"
[params]
tp = 1
[taxonomies]
theme = "themes"
[permalinks.page]
posts = "/:year/:slug/"
[outputs]
home = ["html", "json"]
[menus]
[[menus.main]]
name = "Theme"
url = "/t/"
[[menus.footer]]
name = "Footer"
url = "/f/"
[languages.en.menus]
[[languages.en.menus.side]]
name = "Side"
url = "/s/"
[[languages.en.menus.extra]]
name = "Extra"
url = "/e/"
"#;
    // Defaults: `taxonomies`, `permalinks`, `outputs` are `none`; `menus` `shallow`; root
    // values not merged.
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n[[menus.main]]\nname = \"Mine\"\nurl = \"/\"\n[languages.en]\nweight = 1\n[[languages.en.menus.side]]\nname = \"MySide\"\nurl = \"/ms/\"\n",
        ),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    let s = c.default_site();
    assert_eq!(s.title, "");
    let taxonomies: Vec<&str> = s.taxonomies.iter().map(|t| t.plural.as_str()).collect();
    assert_eq!(taxonomies, ["categories", "tags"]);
    assert_eq!(s.permalinks.get(PageKind::Page, "posts"), None);
    let home: Vec<&str> = s
        .outputs
        .get(PageKind::Home)
        .iter()
        .map(|&id| c.output_formats.get(id).name.as_str())
        .collect();
    assert_eq!(home, ["html", "rss"]);
    let mut menus: Vec<(&str, &str)> = s
        .menus
        .iter()
        .map(|e| (e.menu.as_str(), e.name.as_str()))
        .collect();
    menus.sort_unstable();
    // The language's own menus replace the root's; the theme adds `extra` to them.
    assert_eq!(menus, [("extra", "Extra"), ("side", "MySide")]);

    // `_merge` in the project opens (or closes) a table.
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n[taxonomies]\n_merge = \"deep\"\ntag = \"tags\"\n[permalinks]\n_merge = \"deep\"\n[outputs]\n_merge = \"shallow\"\n[menus]\n_merge = \"none\"\n[[menus.main]]\nname = \"Mine\"\nurl = \"/\"\n",
        ),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    let s = c.default_site();
    let mut taxonomies: Vec<&str> = s.taxonomies.iter().map(|t| t.plural.as_str()).collect();
    taxonomies.sort_unstable();
    assert_eq!(taxonomies, ["tags", "themes"]);
    assert_eq!(
        s.permalinks.get(PageKind::Page, "posts"),
        Some("/:year/:slug/")
    );
    let home: Vec<&str> = s
        .outputs
        .get(PageKind::Home)
        .iter()
        .map(|&id| c.output_formats.get(id).name.as_str())
        .collect();
    assert_eq!(home, ["html", "json"]);
    let menus: Vec<&str> = s.menus.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(menus, ["Mine"]);

    // `_merge = "none"` at the root: nothing from the theme.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n_merge = \"none\"\n"),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    assert!(c.default_site().params.is_empty());
    assert!(c.default_site().menus.is_empty());
}
