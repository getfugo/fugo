//! A purge plan: compiled from a style sheet once, then run against the names of a build.

use super::*;

impl PurgePlan {
    /// Prepares `css` for purging with `options`; `content` are texts (scripts) whose words
    /// count as used names on every page. The pieces are printed for `targets` (the
    /// minifier's, [`crate::Minifier::css_targets`]: without browsers lightningcss prints the
    /// newest syntax, such as media query ranges).
    ///
    /// # Errors
    /// An invalid regular expression, or CSS that cannot be printed.
    pub fn compile(
        css: &str,
        options: &PurgeOptions,
        content: &[&str],
        targets: Targets,
    ) -> Result<Self, MinifyError> {
        // A byte order mark (Sass writes one before non-ASCII CSS) would be read as part of the
        // first selector, and it means nothing inside a page's `<style>`.
        let css = css.strip_prefix('\u{feff}').unwrap_or(css);
        let mut c = Compiler {
            safelist: patterns(&options.safelist)?,
            greedy: patterns(&options.greedy)?,
            blocklist: patterns(&options.blocklist)?,
            content: content.iter().flat_map(|t| words(t)).collect(),
            drop_important: options.drop_important,
            names: Vec::new(),
            blocked: Vec::new(),
            name_ids: HashMap::new(),
            vars: Vec::new(),
            var_ids: HashMap::new(),
            selectors: Vec::new(),
            always_vars: Vec::new(),
            targets,
        };
        let items = match StyleSheet::parse(css, ParserOptions::default()) {
            Ok(sheet) => c.rules(&sheet.rules)?,
            Err(_) => {
                let mut items = Vec::new();
                for (from, to) in crate::css::split(css) {
                    let chunk = &css[from..to];
                    match StyleSheet::parse(chunk, ParserOptions::default()) {
                        Ok(sheet) => items.extend(c.rules(&sheet.rules)?),
                        Err(_) => {
                            let text = chunk.trim().to_owned();
                            let refs = c.refs(&text);
                            c.always_vars.extend(refs);
                            items.push(Item::Raw(text));
                        }
                    }
                }
                items
            }
        };
        let mut always_vars = c.always_vars;
        for (i, v) in c.vars.iter().enumerate() {
            if c.content.contains(v.as_str()) {
                always_vars.push(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        Ok(Self {
            names: c.names,
            vars: c.vars,
            selectors: c.selectors,
            items,
            always_vars,
            variables: options.variables,
        })
    }

    /// The CSS `page` can use, printed compactly.
    #[must_use]
    pub fn purge(&self, page: &PageNames) -> String {
        let used: Vec<bool> = self
            .names
            .iter()
            .map(|n| n.always || page.has(n.kind, &n.text))
            .collect();
        let kept: Vec<bool> = self
            .selectors
            .iter()
            .map(|s| match s.keep {
                Keep::Always => true,
                Keep::Never => false,
                Keep::Check => s.need.met(&used),
            })
            .collect();
        let vars = self.variables.then(|| self.used_vars(&kept, page));
        let mut out = String::new();
        self.write(&self.items, &kept, vars.as_deref(), &mut out);
        out
    }

    /// The custom properties a page needs, given the kept selectors.
    pub(super) fn used_vars(&self, kept: &[bool], page: &PageNames) -> Vec<bool> {
        let mut used = vec![false; self.vars.len()];
        let mut defs: Vec<Vec<u32>> = vec![Vec::new(); self.vars.len()];
        let mut stack = self.always_vars.clone();
        for (i, v) in self.vars.iter().enumerate() {
            if page.words.contains(v) {
                stack.push(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        self.var_roots(&self.items, kept, &mut defs, &mut stack);
        while let Some(v) = stack.pop() {
            let Some(slot) = used.get_mut(v as usize) else {
                continue;
            };
            if !*slot {
                *slot = true;
                stack.extend(&defs[v as usize]);
            }
        }
        used
    }

    pub(super) fn var_roots(
        &self,
        items: &[Item],
        kept: &[bool],
        defs: &mut [Vec<u32>],
        roots: &mut Vec<u32>,
    ) {
        for item in items {
            match item {
                Item::Raw(_) => {}
                Item::Style { sels, decls } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        for d in decls {
                            match d.defines {
                                Some(v) => defs[v as usize].extend(&d.refs),
                                None => roots.extend(&d.refs),
                            }
                        }
                    }
                }
                Item::Opaque { sels, refs, .. } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        roots.extend(refs);
                    }
                }
                Item::Group { items, .. } => self.var_roots(items, kept, defs, roots),
            }
        }
    }

    pub(super) fn write(
        &self,
        items: &[Item],
        kept: &[bool],
        vars: Option<&[bool]>,
        out: &mut String,
    ) {
        for item in items {
            match item {
                Item::Raw(text) => out.push_str(text),
                Item::Opaque { sels, text, .. } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        out.push_str(text);
                    }
                }
                Item::Style { sels, decls } => {
                    let start = out.len();
                    for &s in sels.iter().filter(|&&s| kept[s as usize]) {
                        if out.len() > start {
                            out.push(',');
                        }
                        out.push_str(&self.selectors[s as usize].text);
                    }
                    if out.len() == start {
                        continue;
                    }
                    out.push('{');
                    let body = out.len();
                    for d in decls {
                        if let (Some(v), Some(used)) = (d.defines, vars)
                            && !used[v as usize]
                        {
                            continue;
                        }
                        if out.len() > body {
                            out.push(';');
                        }
                        out.push_str(&d.text);
                    }
                    if out.len() == body {
                        out.truncate(start);
                    } else {
                        out.push('}');
                    }
                }
                Item::Group { prelude, items } => {
                    let start = out.len();
                    if let Some(p) = prelude {
                        out.push_str(p);
                        out.push('{');
                    }
                    let inner = out.len();
                    self.write(items, kept, vars, out);
                    if out.len() == inner {
                        out.truncate(start);
                    } else if prelude.is_some() {
                        out.push('}');
                    }
                }
            }
        }
    }
}
