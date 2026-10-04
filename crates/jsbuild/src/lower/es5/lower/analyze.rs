//! The analysis before lowering: `let` and `const` become `var`s renamed apart, block-level
//! functions, and the wrapper of the bundle.

use super::*;

/// What [`analyze`] decided about block-scoped bindings.
pub(super) struct Plan {
    /// Bindings that would clash once they are `var`s, and their new names.
    pub(super) renames: HashMap<SymbolId, String>,
    /// Function declarations in blocks of strict code (block-scoped since ES2015).
    pub(super) block_fns: HashSet<SymbolId>,
    /// Every name the program declares or references.
    pub(super) used: HashSet<String>,
}

/// Plans turning `let`, `const` and block-level functions into `var`s.
///
/// A binding declared directly in a function (or the program) keeps its name: `let x` there
/// cannot coexist with another `x` in that scope. A binding in a block moves to the enclosing
/// function, so it is renamed when that function (including nested scopes) declares or
/// references another `x`, a binding further out or a global included; the new name is unused
/// in the whole program.
///
/// A binding in a loop is a new binding on every iteration; `var` is one for the whole function.
/// The two only differ when a closure captures the binding, which the generated code Rolldown
/// and oxc emit at `es2015` never does; this is an error rather than a wrong lowering.
pub(super) fn analyze(semantic: &Semantic<'_>) -> Result<Plan, Error> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut used: HashSet<String> = scoping.symbol_names().map(str::to_owned).collect();
    used.extend(
        scoping
            .root_unresolved_references()
            .keys()
            .map(|k| k.as_str().to_owned()),
    );
    let mut by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for s in scoping.symbol_ids() {
        by_name.entry(scoping.symbol_name(s)).or_default().push(s);
    }
    let within = |inner: ScopeId, outer: ScopeId| {
        inner == outer || scoping.scope_is_descendant_of(inner, outer)
    };
    let mut plan = Plan {
        renames: HashMap::new(),
        block_fns: HashSet::new(),
        used,
    };
    for s in scoping.symbol_ids() {
        let flags = scoping.symbol_flags(s);
        let scope = scoping.symbol_scope_id(s);
        let scope_flags = scoping.scope_flags(scope);
        let lexical = flags.contains(SymbolFlags::BlockScopedVariable);
        let block_fn = flags.contains(SymbolFlags::Function)
            && !scope_flags.intersects(ScopeFlags::Var)
            && scope_flags.contains(ScopeFlags::StrictMode)
            && matches!(nodes.kind(scoping.symbol_declaration(s)), AstKind::Function(f) if f.is_declaration());
        if !lexical && !block_fn {
            continue;
        }
        let function = var_scope(scoping, scope);
        let function_node = scoping.get_node_id(function);
        let decl = scoping.symbol_declaration(s);
        let in_loop = nodes
            .ancestors(decl)
            .take_while(|n| n.id() != function_node)
            .any(|n| {
                matches!(
                    n.kind(),
                    AstKind::ForStatement(_)
                        | AstKind::ForInStatement(_)
                        | AstKind::ForOfStatement(_)
                        | AstKind::WhileStatement(_)
                        | AstKind::DoWhileStatement(_)
                )
            });
        if in_loop {
            for &r in scoping.get_resolved_reference_ids(s) {
                let from = scoping.get_reference(r).scope_id();
                let captured = scoping
                    .scope_ancestors(from)
                    .take_while(|&x| x != scope)
                    .any(|x| scoping.scope_flags(x).contains(ScopeFlags::Function));
                if captured {
                    return Err((
                        scoping.symbol_span(s).start,
                        format!(
                            "lower_to_es5: \"{}\" is declared in a loop and captured by a closure; ES5 has no per-iteration bindings",
                            scoping.symbol_name(s)
                        ),
                    ));
                }
            }
        }
        if block_fn {
            plan.block_fns.insert(s);
        }
        if scope == function {
            continue;
        }
        let name = scoping.symbol_name(s);
        let others = by_name.get(name).map_or(&[][..], Vec::as_slice);
        let clash = others.iter().any(|&t| {
            t != s
                && (within(scoping.symbol_scope_id(t), function)
                    || scoping
                        .get_resolved_reference_ids(t)
                        .iter()
                        .any(|&r| within(scoping.get_reference(r).scope_id(), function)))
        }) || scoping
            .root_unresolved_references()
            .iter()
            .filter(|(k, _)| k.as_str() == name)
            .flat_map(|(_, refs)| refs.iter())
            .any(|&r| within(scoping.get_reference(r).scope_id(), function));
        if clash {
            let fresh = fresh_name(&mut plan.used, name);
            plan.renames.insert(s, fresh);
        }
    }
    Ok(plan)
}

/// The function (or program) scope that a `var` in `scope` belongs to.
pub(super) fn var_scope(scoping: &Scoping, mut scope: ScopeId) -> ScopeId {
    loop {
        if scoping.scope_flags(scope).intersects(ScopeFlags::Var) {
            return scope;
        }
        match scoping.scope_parent_id(scope) {
            Some(p) => scope = p,
            None => return scope,
        }
    }
}

/// `base`, or `base2`, `base3`, ... : the first not in `used` (then marked used).
pub(super) fn fresh_name(used: &mut HashSet<String>, base: &str) -> String {
    let mut name = base.to_owned();
    let mut n = 2;
    while used.contains(&name) {
        name = format!("{base}{n}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

/// The span of the function of an IIFE bundle (`(function () { ... })(...)`, possibly
/// assigned to a `var`): helpers go at the top of its body rather than the global scope.
pub(super) fn iife_wrapper(program: &Program<'_>) -> Option<Span> {
    let mut stmts = program
        .body
        .iter()
        .filter(|s| !matches!(s, Statement::EmptyStatement(_)));
    let only = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    let expr = match only {
        Statement::ExpressionStatement(e) => &e.expression,
        Statement::VariableDeclaration(d) if d.declarations.len() == 1 => {
            d.declarations[0].init.as_ref()?
        }
        _ => return None,
    };
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return None;
    };
    match call.callee.without_parentheses() {
        Expression::FunctionExpression(f) if !f.r#async && !f.generator => Some(f.span),
        _ => None,
    }
}
