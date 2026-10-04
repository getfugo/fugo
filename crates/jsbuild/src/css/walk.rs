//! The walk over a parsed style sheet that collects local names, `composes` and their order, and
//! writes the module.

use super::*;

/// A name of the file: local (renamed) or global (kept).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Sym {
    pub(super) name: String,
    pub(super) local: bool,
}

/// The `composes` of one class: names of other files come first in its string.
#[derive(Debug, Default)]
pub(super) struct Composed {
    pub(super) imported: Vec<(String, String)>,
    pub(super) names: Vec<Sym>,
}

/// One part of a class string: a name, or the export of another file's module.
pub(super) enum Part {
    Name(String),
    Import(usize),
}

/// Collects the local names in esbuild's order: a style rule's selectors, then its nested
/// rules, then its declarations; an at-rule's name after its block.
pub(super) struct Walk {
    /// Names are local unless `:global(...)` (`local-css`), else only in `:local(...)`.
    pub(super) local: bool,
    /// Each local name with the rule it first appears in.
    pub(super) locals: Vec<(String, Location)>,
    pub(super) seen: HashSet<String>,
    pub(super) composes: HashMap<Sym, Composed>,
}

impl Walk {
    pub(super) fn name(&mut self, name: &str, local: bool, loc: Location) {
        if local && self.seen.insert(name.to_owned()) {
            self.locals.push((name.to_owned(), loc));
        }
    }

    /// `nested`: inside a style rule.
    pub(super) fn rules(&mut self, rules: &CssRuleList<'_>, nested: bool) {
        for rule in &rules.0 {
            self.rule(rule, nested);
        }
    }

    pub(super) fn rule(&mut self, rule: &CssRule<'_>, nested: bool) {
        match rule {
            CssRule::Style(style) => self.style(style, nested),
            CssRule::Media(r) => self.rules(&r.rules, nested),
            CssRule::Supports(r) => self.rules(&r.rules, nested),
            CssRule::LayerBlock(r) => self.rules(&r.rules, nested),
            CssRule::Scope(r) => self.rules(&r.rules, nested),
            CssRule::StartingStyle(r) => self.rules(&r.rules, nested),
            CssRule::MozDocument(r) => self.rules(&r.rules, nested),
            CssRule::Nesting(r) => self.style(&r.style, true),
            CssRule::Container(r) => {
                self.rules(&r.rules, nested);
                if let Some(name) = &r.name {
                    self.name(&name.0.0, self.local, r.loc);
                }
            }
            CssRule::Keyframes(r) => match &r.name {
                KeyframesName::Ident(name) => self.name(&name.0, self.local, r.loc),
                KeyframesName::Custom(name) => self.name(name, self.local, r.loc),
            },
            CssRule::CounterStyle(r) => self.name(&r.name.0, self.local, r.loc),
            _ => {}
        }
    }

    pub(super) fn style(&mut self, rule: &StyleRule<'_>, nested: bool) {
        for selector in &rule.selectors.0 {
            self.selector(selector, self.local, rule.loc);
        }
        let block = &rule.declarations;
        let mut declarations: Vec<&Property<'_>> = block.declarations.iter().collect();
        declarations.extend(&block.important_declarations);
        for child in &rule.rules.0 {
            match child {
                // Declarations after nested rules still belong to this rule.
                CssRule::NestedDeclarations(r) => {
                    declarations.extend(&r.declarations.declarations);
                    declarations.extend(&r.declarations.important_declarations);
                }
                child => self.rule(child, true),
            }
        }
        // `composes` only works in a top-level rule of single class selectors.
        let parents = if nested {
            None
        } else {
            rule.selectors
                .0
                .iter()
                .map(|s| single_class(s, self.local))
                .collect::<Option<Vec<_>>>()
        };
        for property in declarations {
            self.property(property, parents.as_deref(), rule.loc);
        }
    }

    pub(super) fn selector(&mut self, selector: &Selector<'_>, local: bool, loc: Location) {
        for component in source_order(selector) {
            match component {
                Component::Class(name) | Component::ID(name) => self.name(&name.0, local, loc),
                Component::NonTSPseudoClass(PseudoClass::Local { selector }) => {
                    self.selector(selector, true, loc);
                }
                Component::NonTSPseudoClass(PseudoClass::Global { selector }) => {
                    self.selector(selector, false, loc);
                }
                Component::Negation(list)
                | Component::Is(list)
                | Component::Where(list)
                | Component::Has(list) => {
                    for s in list {
                        self.selector(s, local, loc);
                    }
                }
                Component::NthOf(nth) => {
                    for s in nth.selectors() {
                        self.selector(s, local, loc);
                    }
                }
                // esbuild leaves the arguments of other pseudo-classes and elements alone.
                _ => {}
            }
        }
    }

    pub(super) fn property(
        &mut self,
        property: &Property<'_>,
        parents: Option<&[Sym]>,
        loc: Location,
    ) {
        match property {
            Property::Composes(composes) => {
                if let Some(parents) = parents {
                    self.composes(composes, parents, loc);
                }
            }
            Property::Animation(list, prefix) if *prefix == VendorPrefix::None => {
                for animation in list {
                    self.animation_name(&animation.name, loc);
                }
            }
            Property::AnimationName(list, prefix) if *prefix == VendorPrefix::None => {
                for name in list {
                    self.animation_name(name, loc);
                }
            }
            Property::ListStyle(list) => self.counter_style(&list.list_style_type, loc),
            Property::ListStyleType(list) => self.counter_style(list, loc),
            Property::Container(container) => self.container_names(&container.name, loc),
            Property::ContainerName(names) => self.container_names(names, loc),
            Property::Unparsed(unparsed) if unparsed.property_id.prefix() == VendorPrefix::None => {
                // `animation: spin var(--time)`: esbuild's scan of the shorthand's tokens.
                let id = unparsed.property_id.name();
                if id == "animation" || id == "animation-name" {
                    self.animation_tokens(&unparsed.value.0, id == "animation", loc);
                }
            }
            _ => {}
        }
    }

    pub(super) fn animation_name(&mut self, name: &AnimationName<'_>, loc: Location) {
        match name {
            AnimationName::Ident(name) if !is_keyword(&name.0) => {
                self.name(&name.0, self.local, loc);
            }
            AnimationName::String(name) => self.name(&name.0, self.local, loc),
            _ => {}
        }
    }

    /// esbuild's `processAnimationShorthand` (`processAnimationName` when not `shorthand`):
    /// the first identifier of each comma-separated animation that is not a keyword of
    /// another longhand is its name.
    pub(super) fn animation_tokens(
        &mut self,
        tokens: &[TokenOrValue<'_>],
        shorthand: bool,
        loc: Location,
    ) {
        let mut found = [false; 6]; // timing, iteration count, direction, fill, play state, name
        for token in tokens {
            match token {
                TokenOrValue::Token(Token::Comma) => found = [false; 6],
                TokenOrValue::Token(Token::Number { .. }) if shorthand => found[1] = true,
                TokenOrValue::Token(Token::String(name)) if !shorthand || !found[5] => {
                    found[5] = true;
                    self.name(name, self.local, loc);
                }
                TokenOrValue::Token(Token::Ident(ident)) => {
                    if !shorthand {
                        if !is_keyword(ident) {
                            self.name(ident, self.local, loc);
                        }
                        continue;
                    }
                    let lower = ident.to_ascii_lowercase();
                    let slot = match lower.as_str() {
                        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
                        | "step-start" | "step-end"
                            if !found[0] =>
                        {
                            0
                        }
                        "infinite" if !found[1] => 1,
                        "normal" | "reverse" | "alternate" | "alternate-reverse" if !found[2] => 2,
                        "none" | "forwards" | "backwards" | "both" if !found[3] => 3,
                        "running" | "paused" if !found[4] => 4,
                        _ if !found[5] => {
                            if !is_keyword(ident) {
                                self.name(ident, self.local, loc);
                            }
                            5
                        }
                        _ => continue,
                    };
                    found[slot] = true;
                }
                _ => {}
            }
        }
    }

    pub(super) fn counter_style(&mut self, list: &ListStyleType<'_>, loc: Location) {
        if let ListStyleType::CounterStyle(CounterStyle::Name(name)) = list
            && !is_keyword(&name.0)
        {
            self.name(&name.0, self.local, loc);
        }
    }

    pub(super) fn container_names(&mut self, names: &ContainerNameList<'_>, loc: Location) {
        if let ContainerNameList::Names(names) = names {
            for name in names {
                if !is_keyword(&name.0.0) {
                    self.name(&name.0.0, self.local, loc);
                }
            }
        }
    }

    pub(super) fn composes(&mut self, composes: &Composes<'_>, parents: &[Sym], loc: Location) {
        for parent in parents {
            for name in &composes.names {
                let name = name.0.to_string();
                let entry = self.composes.entry(parent.clone()).or_default();
                match &composes.from {
                    None => {
                        entry.names.push(Sym {
                            name: name.clone(),
                            local: self.local,
                        });
                        self.name(&name, self.local, loc);
                    }
                    Some(Specifier::Global) => entry.names.push(Sym { name, local: false }),
                    // Like esbuild, `composes` from an external URL is ignored.
                    Some(Specifier::File(file)) if !is_external(file) => {
                        entry.imported.push((file.to_string(), name));
                    }
                    Some(_) => {}
                }
            }
        }
    }

    /// What `root` composes, before its own name: esbuild's depth-first walk of `composes`,
    /// each name once, names of other files first at each class. Other files' strings come
    /// from their modules (indexes into `imports`).
    pub(super) fn composed(
        &self,
        root: &Sym,
        prefix: &str,
        imports: &mut Vec<(String, String)>,
    ) -> Vec<Part> {
        let mut visited = HashSet::from([root.clone()]);
        let mut parts = Vec::new();
        self.visit(root, prefix, imports, &mut visited, &mut parts);
        parts
    }

    pub(super) fn visit(
        &self,
        sym: &Sym,
        prefix: &str,
        imports: &mut Vec<(String, String)>,
        visited: &mut HashSet<Sym>,
        parts: &mut Vec<Part>,
    ) {
        let Some(composed) = self.composes.get(sym) else {
            return;
        };
        for import in &composed.imported {
            let index = match imports.iter().position(|i| i == import) {
                Some(i) => i,
                None => {
                    imports.push(import.clone());
                    imports.len() - 1
                }
            };
            parts.push(Part::Import(index));
        }
        for name in &composed.names {
            if visited.insert(name.clone()) {
                self.visit(name, prefix, imports, visited, parts);
                parts.push(Part::Name(generated(name, prefix)));
            }
        }
    }

    /// The ES module: one variable per local name, the default export object, named exports.
    pub(super) fn module_js(&self, prefix: &str) -> String {
        if self.locals.is_empty() {
            return EMPTY_MODULE.to_owned();
        }
        let mut imports = Vec::new();
        let mut vars = String::new();
        let mut needs_join = false;
        for (i, (name, _)) in self.locals.iter().enumerate() {
            let sym = Sym {
                name: name.clone(),
                local: true,
            };
            let own = generated(&sym, prefix);
            let parts = self.composed(&sym, prefix, &mut imports);
            let value = if parts.iter().any(|p| matches!(p, Part::Import(_))) {
                // Deduplicated when the module runs, against the other modules' strings.
                needs_join = true;
                let parts: Vec<String> = parts.iter().map(part_js).collect();
                format!("__ssg_compose({}, [{}])", js_string(&own), parts.join(", "))
            } else {
                let mut names: Vec<&str> = parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Name(n) => Some(n.as_str()),
                        Part::Import(_) => None,
                    })
                    .collect();
                names.push(&own);
                js_string(&names.join(" "))
            };
            vars.push_str(&format!("var c{i} = {value};\n"));
        }

        let mut js = String::new();
        for (i, (file, name)) in imports.iter().enumerate() {
            js.push_str(&format!(
                "import {{ {} as i{i} }} from {};\n",
                export_name(name),
                js_string(file)
            ));
        }
        js.push_str(&vars);
        let entries: Vec<String> = self
            .locals
            .iter()
            .enumerate()
            .map(|(i, (name, _))| format!("{}: c{i}", js_string(name)))
            .collect();
        js.push_str(&format!("export default {{ {} }};\n", entries.join(", ")));
        let named: Vec<String> = self
            .locals
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name != "default")
            .map(|(i, (name, _))| format!("c{i} as {}", export_name(name)))
            .collect();
        if !named.is_empty() {
            js.push_str(&format!("export {{ {} }};\n", named.join(", ")));
        }
        if needs_join {
            js.push_str(COMPOSE_JS);
        }
        js
    }
}
