//! What the browser is told after a rebuild: reload, or go to the page that changed.

use super::*;

impl Rebuilder {
    pub(super) fn reload_after_build(
        &self,
        changes: &Changes,
        changed: Option<&[String]>,
        report: &BuildReport,
    ) {
        if changed.is_some_and(<[String]>::is_empty) {
            return;
        }
        if !changes.content.is_empty() {
            let navigate = self.live_reload.is_some_and(|l| l.navigate_to_changed);
            match self.changed_page(changes, report).filter(|_| navigate) {
                Some((path, port)) => self.send(&livereload::navigate(&path, port)),
                None => self.send(&livereload::force_refresh()),
            }
            return;
        }
        let Some(changed) = changed else {
            self.send(&livereload::force_refresh());
            return;
        };
        let (css, other): (Vec<&String>, Vec<&String>) =
            changed.iter().partition(|p| p.ends_with(".css"));
        if other.len() == 1 {
            self.send(&livereload::reload(&self.url_path(other[0])));
        } else if css.is_empty() || other.len() > 1 {
            self.send(&livereload::force_refresh());
        }
        if !css.is_empty() {
            if !other.is_empty() {
                // Let the reloaded pages connect again first (Go waits as long).
                std::thread::sleep(Duration::from_millis(200));
            }
            for c in css {
                self.send(&livereload::reload(&self.url_path(c)));
            }
        }
    }

    /// The page of the content file a batch wrote or created (an index file first), with the
    /// port of its language's server: Go's `pickOneWriteOrCreatePath`.
    pub(super) fn changed_page(
        &self,
        changes: &Changes,
        report: &BuildReport,
    ) -> Option<(String, Option<u16>)> {
        let cfg = &self.cfg;
        let is_content = |p: &Path| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| cfg.content_types.is_content_suffix(&cfg.media_types, e))
        };
        let is_index = |p: &Path| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("index.") || n.starts_with("_index."))
        };
        let mut file = None;
        for (p, written) in &changes.content {
            if *written && is_content(p) {
                file = Some(p);
                if is_index(p) {
                    break;
                }
            }
        }
        let file = file?;
        let model = report.model.as_ref()?;
        let page = model
            .pages
            .iter()
            .find(|p| p.source.as_ref().is_some_and(|s| s.file.abs == *file))?;
        let links = page.urls.first()?.links.as_ref()?;
        let url = UrlRef::parse(links.permalink.as_str()).ok()?;
        let port = cfg.sites.get(page.lang).and_then(|s| s.base_url.port());
        Some((url.escaped_path().into_owned(), port))
    }

    /// The URL path of a file of the served tree, on its listener.
    pub(super) fn url_path(&self, file: &str) -> String {
        let served = self.shared.served();
        served
            .hosts
            .iter()
            .find_map(|h| {
                file.strip_prefix(h.root.as_str())
                    .map(|rest| format!("{}{rest}", h.base_path))
            })
            .unwrap_or_else(|| format!("/{file}"))
    }

    pub(super) fn send(&self, command: &str) {
        self.reporter.report(&Event::Reload { command });
        let _ = self.shared.reload.send(Arc::from(command));
    }

    pub(super) fn error(&self, message: &str) {
        self.reporter.report(&Event::Error { message });
    }
}
