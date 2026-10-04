//! Scans of the AST (what needs lowering, the names in use, `this` and `super` in code that moves)
//! and the renaming passes.

use super::*;

/// Byte offsets of line starts, for error positions. Line terminators are those of JavaScript.
pub(super) struct LineIndex<'s> {
    pub(super) source: &'s str,
    pub(super) starts: Vec<u32>,
}

impl<'s> LineIndex<'s> {
    pub(super) fn new(source: &'s str) -> Self {
        let mut starts = vec![0];
        let mut chars = source.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let next = match c {
                '\r' if chars.peek().is_some_and(|&(_, n)| n == '\n') => continue,
                '\n' | '\r' | '\u{2028}' | '\u{2029}' => i + c.len_utf8(),
                _ => continue,
            };
            starts.push(u32::try_from(next).unwrap_or(u32::MAX));
        }
        Self { source, starts }
    }

    pub(super) fn error(&self, offset: u32, message: impl Into<String>) -> LowerError {
        let offset = offset.min(u32::try_from(self.source.len()).unwrap_or(u32::MAX));
        let line = self.starts.partition_point(|&s| s <= offset).max(1);
        LowerError {
            message: message.into(),
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: offset - self.starts[line - 1],
        }
    }
}

/// Whether a program has a decorator or an auto-accessor, the only things this module changes.
pub(super) struct NeedsLowering(pub(super) bool);

impl<'a> Visit<'a> for NeedsLowering {
    fn visit_decorator(&mut self, _: &Decorator<'a>) {
        self.0 = true;
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        self.0 = true;
        walk::walk_accessor_property(self, it);
    }
}

/// Every identifier name of a program, which fresh names must avoid.
pub(super) struct NameCollector<'n>(pub(super) &'n mut HashSet<String>);

impl<'a> Visit<'a> for NameCollector<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.0.insert(it.name.as_str().to_owned());
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.insert(it.name.as_str().to_owned());
    }
}

/// Whether the class body uses `this` or `super` in code that moves out of the class
/// (static initializers and blocks) or `super` in a lowered private method.
pub(super) fn uses_moved_this_or_super<'a>(
    e: &ClassElement<'a>,
    privates: &HashMap<&'a str, PrivateLowering<'a>>,
) -> bool {
    let mut scan = ThisSuperScan::default();
    match e {
        ClassElement::StaticBlock(b) => {
            for s in &b.body {
                scan.visit_statement(s);
            }
            scan.this || scan.sup
        }
        ClassElement::PropertyDefinition(p) if p.r#static => {
            if let Some(v) = &p.value {
                scan.visit_expression(v);
            }
            scan.this || scan.sup
        }
        ClassElement::AccessorProperty(p) if p.r#static => {
            if let Some(v) = &p.value {
                scan.visit_expression(v);
            }
            scan.this || scan.sup
        }
        ClassElement::MethodDefinition(m) => {
            let PropertyKey::PrivateIdentifier(p) = &m.key else {
                return false;
            };
            if !privates.contains_key(p.name.as_str()) {
                return false;
            }
            if let Some(body) = &m.value.body {
                for s in &body.statements {
                    scan.visit_statement(s);
                }
            }
            scan.sup
        }
        _ => false,
    }
}

/// Finds `this` and `super` that belong to the enclosing code (not to nested functions).
#[derive(Default)]
pub(super) struct ThisSuperScan {
    pub(super) this: bool,
    pub(super) sup: bool,
}

impl<'a> Visit<'a> for ThisSuperScan {
    fn visit_this_expression(&mut self, _: &ThisExpression) {
        self.this = true;
    }

    fn visit_super(&mut self, _: &Super) {
        self.sup = true;
    }

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        for d in &it.decorators {
            self.visit_decorator(d);
        }
        if let Some(h) = &it.heritage {
            self.visit_expression(&h.expression);
        }
        for e in &it.body.body {
            if let Some(key) = element_key(e).and_then(PropertyKey::as_expression) {
                self.visit_expression(key);
            }
            for d in element_decorators(e) {
                self.visit_decorator(d);
            }
        }
    }
}

/// Finds a reference to a class binding.
pub(super) struct InnerRefScan<'s> {
    pub(super) scoping: &'s Scoping,
    pub(super) symbol: Option<SymbolId>,
    pub(super) found: bool,
}

impl<'a> Visit<'a> for InnerRefScan<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if let (Some(symbol), Some(r)) = (self.symbol, it.reference_id.get())
            && self.scoping.get_reference(r).symbol_id() == Some(symbol)
        {
            self.found = true;
        }
    }
}

/// Finds a reference named `name` (any binding).
pub(super) struct NameScan<'n> {
    pub(super) name: &'n str,
    pub(super) found: bool,
}

impl<'a> Visit<'a> for NameScan<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.name.as_str() == self.name {
            self.found = true;
        }
    }
}

/// Renames the references to a class binding.
pub(super) struct Renamer<'s, 'a> {
    pub(super) scoping: &'s Scoping,
    pub(super) symbol: SymbolId,
    pub(super) to: &'a str,
}

impl<'a> VisitMut<'a> for Renamer<'_, 'a> {
    fn visit_identifier_reference(&mut self, it: &mut IdentifierReference<'a>) {
        if let Some(r) = it.reference_id.get()
            && self.scoping.get_reference(r).symbol_id() == Some(self.symbol)
        {
            it.name = self.to.into();
        }
    }
}

/// Counts `super(...)` calls that belong to a constructor (arrow functions included).
#[derive(Default)]
pub(super) struct SuperCalls {
    pub(super) count: usize,
}

impl<'a> Visit<'a> for SuperCalls {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if it.callee.is_super() {
            self.count += 1;
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(h) = &it.heritage {
            self.visit_expression(&h.expression);
        }
        for e in &it.body.body {
            if let Some(key) = element_key(e).and_then(PropertyKey::as_expression) {
                self.visit_expression(key);
            }
        }
    }
}

/// Replaces `super(...)` calls of a constructor with calls of `to` (esbuild's `__super` shim).
pub(super) struct SuperCallRenamer<'a> {
    pub(super) to: &'a str,
    pub(super) b: &'a Allocator,
}

impl<'a> VisitMut<'a> for SuperCallRenamer<'a> {
    fn visit_call_expression(&mut self, it: &mut CallExpression<'a>) {
        if it.callee.is_super() {
            it.callee = Expression::new_identifier(SPAN, self.to, &AstBuilder::new(self.b));
        }
        walk_mut::walk_call_expression(self, it);
    }

    fn visit_function(&mut self, _: &mut Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &mut Class<'a>) {
        if let Some(h) = &mut it.heritage {
            self.visit_expression(&mut h.expression);
        }
        for e in &mut it.body.body {
            if let Some(key) = element_key_mut(e).and_then(PropertyKey::as_expression_mut) {
                self.visit_expression(key);
            }
        }
    }
}
