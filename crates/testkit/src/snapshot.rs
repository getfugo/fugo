//! Shared insta settings (REWRITE_PLAN.md §7.1): snapshots are reviewed with
//! `INSTA_UPDATE=always` plus `git diff`.

/// The workspace's snapshot settings: maps sorted, and snapshot names without the module prefix.
///
/// Bind them around a test body: `ssg_testkit::snapshot::settings().bind(|| { ... })`.
#[must_use]
pub fn settings() -> insta::Settings {
    let mut s = insta::Settings::clone_current();
    s.set_sort_maps(true);
    s.set_prepend_module_to_snapshot(false);
    s
}
