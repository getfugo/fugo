//! The compiler of a purge plan: rules, selectors and declarations, with what each needs kept.

use super::*;

impl Compiler<'_> {
    pub(super) fn print<T: ToCss>(&self, v: &T) -> Result<String, MinifyError> {
        v.to_css_string(printer(self.targets))
            .map_err(|e| MinifyError::Purge(e.to_string()))
    }

    pub(super) fn rules(&mut self, rules: &CssRuleList<'_>) -> Result<Vec<Item>, MinifyError> {
        rules.0.iter().map(|r| self.rule(r)).collect()
    }

    pub(super) fn rule(&mut self, rule: &CssRule<'_>) -> Result<Item, MinifyError> {
        // A group rule: its prelude (the rule printed without its rules) and its items.
        macro_rules! group {
            ($variant:ident, $r:expr) => {{
                let mut empty = $r.clone();
                empty.rules = CssRuleList(Vec::new());
                let head = self.print(&CssRule::$variant(empty))?;
                match head.strip_suffix("{}") {
                    Some(prelude) => Item::Group {
                        prelude: Some(prelude.to_owned()),
                        items: self.rules(&$r.rules)?,
                    },
                    None if head.is_empty() => Item::Group {
                        prelude: None,
                        items: self.rules(&$r.rules)?,
                    },
                    None => self.raw(rule)?,
                }
            }};
        }
        Ok(match rule {
            CssRule::Style(s) if s.rules.0.is_empty() && s.vendor_prefix.is_empty() => {
                let sels = s
                    .selectors
                    .0
                    .iter()
                    .map(|sel| self.selector(sel))
                    .collect::<Result<_, _>>()?;
                Item::Style {
                    sels,
                    decls: self.declarations(&s.declarations)?,
                }
            }
            CssRule::Style(s) => {
                let sels = s
                    .selectors
                    .0
                    .iter()
                    .map(|sel| self.selector(sel))
                    .collect::<Result<_, _>>()?;
                let text = self.print(rule)?;
                let refs = self.refs(&text);
                Item::Opaque { sels, text, refs }
            }
            CssRule::Media(r) => group!(Media, r),
            CssRule::Supports(r) => group!(Supports, r),
            CssRule::Container(r) => group!(Container, r),
            CssRule::LayerBlock(r) => group!(LayerBlock, r),
            CssRule::MozDocument(r) => group!(MozDocument, r),
            CssRule::Scope(r) => group!(Scope, r),
            CssRule::StartingStyle(r) => group!(StartingStyle, r),
            _ => self.raw(rule)?,
        })
    }

    pub(super) fn raw(&mut self, rule: &CssRule<'_>) -> Result<Item, MinifyError> {
        let text = self.print(rule)?;
        let refs = self.refs(&text);
        self.always_vars.extend(refs);
        Ok(Item::Raw(text))
    }

    pub(super) fn selector(&mut self, sel: &Selector<'_>) -> Result<u32, MinifyError> {
        let text = self.print(sel)?;
        let need = self.need(sel);
        let keep = if need.any_name(&|n| self.blocked[n as usize]) {
            Keep::Never
        } else if self.greedy.iter().any(|g| g.in_text(&text)) {
            Keep::Always
        } else {
            Keep::Check
        };
        self.selectors.push(Sel { text, need, keep });
        Ok(u32::try_from(self.selectors.len() - 1).unwrap_or(u32::MAX))
    }

    pub(super) fn need(&mut self, sel: &Selector<'_>) -> Need {
        let mut all = Vec::new();
        for c in sel.iter_raw_match_order() {
            match c {
                Component::Class(n) => all.push(Need::Name(self.name(NameKind::Class, &n.0))),
                Component::ID(n) => all.push(Need::Name(self.name(NameKind::Id, &n.0))),
                Component::LocalName(n) => {
                    all.push(Need::Name(self.name(NameKind::Tag, &n.lower_name.0)));
                }
                Component::Is(list)
                | Component::Where(list)
                | Component::Has(list)
                | Component::Any(_, list) => {
                    all.push(Need::Any(list.iter().map(|s| self.need(s)).collect()));
                }
                _ => {}
            }
        }
        Need::All(all)
    }

    pub(super) fn name(&mut self, kind: NameKind, text: &str) -> u32 {
        if let Some(&id) = self.name_ids.get(&(kind, text.to_owned())) {
            return id;
        }
        let always = self.safelist.iter().any(|p| p.is_name(text)) || self.content.contains(text);
        self.blocked
            .push(self.blocklist.iter().any(|p| p.is_name(text)));
        self.names.push(Name {
            kind,
            text: text.to_owned(),
            always,
        });
        let id = u32::try_from(self.names.len() - 1).unwrap_or(u32::MAX);
        self.name_ids.insert((kind, text.to_owned()), id);
        id
    }

    pub(super) fn var(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.var_ids.get(name) {
            return id;
        }
        self.vars.push(name.to_owned());
        let id = u32::try_from(self.vars.len() - 1).unwrap_or(u32::MAX);
        self.var_ids.insert(name.to_owned(), id);
        id
    }

    pub(super) fn refs(&mut self, text: &str) -> Vec<u32> {
        var_refs(text).map(|v| self.var(v)).collect()
    }

    pub(super) fn declarations(
        &mut self,
        block: &DeclarationBlock<'_>,
    ) -> Result<Vec<Decl>, MinifyError> {
        let important = !self.drop_important;
        let all = block
            .declarations
            .iter()
            .map(|p| (p, false))
            .chain(block.important_declarations.iter().map(|p| (p, important)));
        let mut decls = Vec::new();
        for (p, important) in all {
            let text = p
                .to_css_string(important, printer(self.targets))
                .map_err(|e| MinifyError::Purge(e.to_string()))?;
            let defines = match p {
                Property::Custom(c) => match &c.name {
                    CustomPropertyName::Custom(d) => Some(self.var(&d.0)),
                    CustomPropertyName::Unknown(_) => None,
                },
                _ => None,
            };
            let refs = self.refs(&text);
            decls.push(Decl {
                text,
                defines,
                refs,
            });
        }
        Ok(decls)
    }
}
