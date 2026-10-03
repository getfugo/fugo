//! URLs and language: `abs_url`, `rel_url`, `abs_lang_url`, `rel_lang_url`, `ref`, `rel_ref`,
//! `i18n`.

use std::sync::Arc;

use ssg_base::LangIdx;
use ssg_base::diag::{Diagnostic, Diagnostics};
use ssg_config::site::RefLinksLevel;
use ssg_locale::{Args, PluralCount, Translations};
use ssg_site::{RefArgs, RefLink};
use ssg_view::ViewCache;
use tera::{Kwargs, State, TeraResult, Value};

use crate::Handles;
use crate::call::{
    Registrar, SiteFilter, SiteFunction, chain, context_lang, msg, page_id, page_scope,
    render_lang, scope, text, to_data,
};

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    for (name, kind) in [
        ("abs_url", UrlKind::Abs),
        ("rel_url", UrlKind::Rel),
        ("abs_lang_url", UrlKind::AbsLang),
        ("rel_lang_url", UrlKind::RelLang),
    ] {
        r.filter(
            name,
            Url {
                views: Arc::clone(&h.views),
                name,
                kind,
            },
        );
    }
    r.function(
        "ref",
        Ref {
            views: Arc::clone(&h.views),
            diagnostics: Arc::clone(&h.diagnostics),
            link: RefLink::Permalink,
        },
    );
    r.function(
        "rel_ref",
        Ref {
            views: Arc::clone(&h.views),
            diagnostics: Arc::clone(&h.diagnostics),
            link: RefLink::RelPermalink,
        },
    );
    r.function(
        "i18n",
        I18n {
            views: Arc::clone(&h.views),
            i18n: Arc::clone(&h.i18n),
        },
    );
}

#[derive(Clone, Copy)]
enum UrlKind {
    Abs,
    Rel,
    AbsLang,
    RelLang,
}

/// `abs_url`, `rel_url` (the render's language: its base URL), `abs_lang_url`,
/// `rel_lang_url` (with the language prefix of `page`'s language, else the render's).
struct Url {
    views: Arc<ViewCache>,
    name: &'static str,
    kind: UrlKind,
}

impl SiteFilter for Url {
    fn call(&self, v: Value, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let input = text(&v, self.name)?;
        let model = self.views.model();
        let lang = match kw.get::<Value>("page")? {
            Some(p) if !p.is_none() => model.pages[page_id(&self.views, &p, self.name)?].lang,
            _ => render_lang(model, st)?,
        };
        let urls = model.config.sites[lang].site_urls();
        Ok(Value::from(match self.kind {
            UrlKind::Abs => urls.abs_url(&input),
            UrlKind::Rel => urls.rel_url(&input),
            UrlKind::AbsLang => urls.abs_lang_url(&input),
            UrlKind::RelLang => urls.rel_lang_url(&input),
        }))
    }
}

/// `ref(path=, lang=?, output_format=?, page=?)` and `rel_ref(…)`: the link of the page at
/// `path` (relative paths and page names resolve from `page`, else the render's page). A
/// reference that does not resolve is reported at `refLinksErrorLevel` and links to
/// `refLinksNotFoundURL`.
struct Ref {
    views: Arc<ViewCache>,
    diagnostics: Arc<Diagnostics>,
    link: RefLink,
}

impl SiteFunction for Ref {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = match self.link {
            RefLink::Permalink => "ref",
            RefLink::RelPermalink => "rel_ref",
        };
        let model = self.views.model();
        let args = RefArgs {
            path: kw.must_get::<&str>("path")?.to_owned(),
            lang: kw.get::<&str>("lang")?.map(str::to_owned),
            output_format: kw.get::<&str>("output_format")?.map(str::to_owned),
        };
        let (lang, from): (LangIdx, _) = match (kw.get::<Value>("page")?, scope(st)?) {
            (Some(p), _) if !p.is_none() => {
                let id = page_id(&self.views, &p, name)?;
                (model.pages[id].lang, Some(id))
            }
            (_, Some(s)) => (s.lang, Some(s.page)),
            _ => (render_lang(model, st)?, None),
        };
        match model.ref_link(lang, &args, from, self.link) {
            Ok(link) => Ok(Value::from(link)),
            Err(e) => {
                let cfg = &model.config.sites[lang].ref_links;
                let at = from.map_or_else(String::new, |p| {
                    format!(" (from page {})", model.pages[p].path())
                });
                let d = match cfg.level {
                    RefLinksLevel::Error => Diagnostic::error(format!("{name}: {e}{at}")),
                    RefLinksLevel::Warning => Diagnostic::warning(format!("{name}: {e}{at}")),
                };
                self.diagnostics.push(d);
                Ok(Value::from(cfg.not_found_url.as_str()))
            }
        }
    }
}

/// `i18n(key=, count=?, data=?, page=?)`: the translation in `page`'s language (else the
/// render's, else the context's `lang`); `count` picks the plural form (else the count of
/// `data`), `data` fills `{{ . }}` and `{{ .Field }}`.
struct I18n {
    views: Arc<ViewCache>,
    i18n: Arc<Translations>,
}

impl SiteFunction for I18n {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let key = kw.must_get::<&str>("key")?;
        let model = self.views.model();
        let lang = match page_scope(&self.views, st, kw, "i18n") {
            Ok(s) => s.lang,
            Err(e) => context_lang(model, st).ok_or(e)?,
        };
        let data = kw
            .get::<Value>("data")?
            .filter(|d| !d.is_none())
            .map(|d| to_data(&d));
        let count = match kw.get::<Value>("count")? {
            Some(c) if !c.is_none() => Some(
                PluralCount::from_value(&to_data(&c))
                    .ok_or_else(|| msg(format!("i18n(count=): not a number: {c}")))?,
            ),
            _ => data.as_ref().and_then(PluralCount::from_value),
        };
        let args = Args {
            count,
            data: data.as_ref(),
        };
        let s = self
            .i18n
            .translate(lang, key, &args)
            .map_err(|e| chain(format!("i18n(key=\"{key}\")"), e))?;
        Ok(Value::from(s))
    }
}
