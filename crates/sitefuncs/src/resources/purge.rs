//! `css.Purge`: the unused rules of a style sheet dropped, given the names a build's output uses.

use super::*;

/// `css | purge_css(safelist=, greedy=, blocklist=, content=, variables=, important=)`: a
/// placeholder the publisher replaces, in each page, with the rules of `css` (a resource or a
/// string) that page uses (`ssg_minify::purge`). The plan is compiled once per input and
/// options.
pub(super) struct PurgeCss {
    pub(super) store: Arc<ResourceStore>,
    pub(super) purges: Arc<CssPurges>,
}

/// A text `purge_css` reads: a resource's content (read when the plan is compiled) or a string.
#[derive(Hash)]
pub(super) enum PurgeText {
    Resource(u32),
    Text(String),
}

impl PurgeCss {
    pub(super) fn source(&self, v: &Value, what: &str) -> TeraResult<PurgeText> {
        if v.as_str().is_some() {
            return Ok(PurgeText::Text(text(v, what)?));
        }
        if is_post_processed(v) {
            return Err(msg(format!(
                "{what}: a post-processed resource has no content before every page is rendered"
            )));
        }
        Ok(PurgeText::Resource(
            resource_id(&self.store, v, what)?.raw(),
        ))
    }

    pub(super) fn read(&self, t: &PurgeText) -> Result<String, MinifyError> {
        match t {
            PurgeText::Text(s) => Ok(s.clone()),
            PurgeText::Resource(raw) => {
                let id = ResourceId::from_raw(*raw);
                let bytes = self
                    .store
                    .content(id)
                    .map_err(|e| MinifyError::Purge(e.to_string()))?;
                String::from_utf8(bytes.to_vec()).map_err(|_| {
                    MinifyError::Purge(format!("{} is not text", self.store.resource(id).name))
                })
            }
        }
    }
}

impl SiteFilter for PurgeCss {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let name = "purge_css";
        let strings = |key: &str| -> TeraResult<Vec<String>> {
            match kw.get::<Value>(key)? {
                Some(v) if !v.is_none() => list(&v, &format!("{name}: {key}"))?
                    .iter()
                    .map(|s| text(s, &format!("{name}: {key}")))
                    .collect(),
                _ => Ok(Vec::new()),
            }
        };
        let options = PurgeOptions {
            safelist: strings("safelist")?,
            greedy: strings("greedy")?,
            blocklist: strings("blocklist")?,
            variables: kw.get::<bool>("variables")?.unwrap_or(false),
            drop_important: !kw.get::<bool>("important")?.unwrap_or(true),
        };
        let css = self.source(&v, name)?;
        let content = match kw.get::<Value>("content")? {
            Some(c) if !c.is_none() => list(&c, &format!("{name}: content"))?
                .iter()
                .map(|item| self.source(item, &format!("{name}: content")))
                .collect::<TeraResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        let mut key = DefaultHasher::new();
        (&options, &css, &content).hash(&mut key);
        let placeholder = self
            .purges
            .placeholder(key.finish(), || {
                let css = self.read(&css)?;
                let content = content
                    .iter()
                    .map(|t| self.read(t))
                    .collect::<Result<Vec<_>, _>>()?;
                let content: Vec<&str> = content.iter().map(String::as_str).collect();
                let targets = self.store.config().transforms.minifier.css_targets();
                PurgePlan::compile(&css, &options, &content, targets)
            })
            .map_err(msg)?;
        Ok(Value::safe_string(&placeholder))
    }
}
