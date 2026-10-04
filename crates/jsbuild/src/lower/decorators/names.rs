//! Names and `var` scopes: fresh temporaries, their declarations, and the context of functions.

use super::*;

impl<'a> Lowerer<'a, '_> {
    pub(super) fn arena_str(&self, s: &str) -> &'a str {
        self.alloc.alloc_str(s)
    }

    /// A name not used anywhere in the program, derived from `base` (made a valid identifier).
    pub(super) fn fresh(&mut self, base: &str) -> &'a str {
        let mut name: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            name.insert(0, '_');
        }
        let mut candidate = name.clone();
        let mut n = 2;
        while self.used_names.contains(&candidate) || is_reserved_keyword(&candidate) {
            candidate = format!("{name}{n}");
            n += 1;
        }
        self.used_names.insert(candidate.clone());
        self.arena_str(&candidate)
    }

    /// A fresh `var` in the current scope, named like `base`.
    pub(super) fn temp_named(&mut self, base: &str) -> &'a str {
        let name = self.fresh(base);
        self.declare(name);
        name
    }

    /// A fresh `var` in the current scope named `_a`, `_b`, ... (esbuild's naming).
    pub(super) fn temp(&mut self) -> &'a str {
        loop {
            let mut n = self.temp_count;
            self.temp_count += 1;
            let mut suffix = String::new();
            loop {
                suffix.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
                if n == 0 {
                    break;
                }
                n -= 1;
            }
            let name = format!("_{suffix}");
            if !self.used_names.contains(&name) {
                self.used_names.insert(name.clone());
                let name = self.arena_str(&name);
                self.declare(name);
                return name;
            }
        }
    }

    pub(super) fn declare(&mut self, name: &'a str) {
        if let Some(scope) = self.var_scopes.last_mut() {
            scope.push(name);
        }
    }

    /// A `var` statement declaring `names`.
    pub(super) fn var_decl(&self, names: &[&'a str]) -> Statement<'a> {
        let decls = ArenaVec::from_iter_in(
            names.iter().map(|n| {
                VariableDeclarator::new(
                    SPAN,
                    BindingPattern::new_binding_identifier(SPAN, *n, &self.b),
                    None,
                    None,
                    false,
                    &self.b,
                )
            }),
            &self.b,
        );
        Statement::new_variable_declaration(
            SPAN,
            VariableDeclarationKind::Var,
            decls,
            false,
            &self.b,
        )
    }

    pub(super) fn with_var_scope(&mut self, f: impl FnOnce(&mut Self)) -> Vec<&'a str> {
        self.var_scopes.push(Vec::new());
        f(self);
        self.var_scopes.pop().unwrap_or_default()
    }

    pub(super) fn finish_var_scope(
        &self,
        temps: &[&'a str],
        stmts: &mut ArenaVec<'a, Statement<'a>>,
    ) {
        if !temps.is_empty() {
            stmts.insert(0, self.var_decl(temps));
        }
    }

    pub(super) fn visit_function_with_ctx(&mut self, func: &mut Function<'a>, ctx: FnCtx<'a>) {
        let old = std::mem::replace(&mut self.fn_ctx, ctx);
        self.check_params(&func.params);
        // Parameters see the enclosing `var` scope: a body `var` is not visible to them.
        self.visit_formal_parameters(&mut func.params);
        if let Some(body) = &mut func.body {
            let temps = self.with_var_scope(|s| s.visit_function_body(body));
            self.finish_var_scope(&temps, &mut body.statements);
        }
        self.fn_ctx = old;
    }

    pub(super) fn check_params(&mut self, params: &FormalParameters<'a>) {
        let spans: Vec<Span> = params
            .items
            .iter()
            .flat_map(|p| p.decorators.iter().map(|d| d.span))
            .chain(
                params
                    .rest
                    .iter()
                    .flat_map(|r| r.decorators.iter().map(|d| d.span)),
            )
            .collect();
        for span in spans {
            self.error(
                span,
                "Parameter decorators only work when experimental decorators are enabled",
            );
        }
    }
}
