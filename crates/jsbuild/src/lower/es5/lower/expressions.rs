//! Templates, spreads, regular expressions, BigInts and `import()`.

use super::*;

impl<'a> Lowerer<'a> {
    /// esbuild's lowering: `"a".concat(b, "c").concat(d)`, one call per substitution so that
    /// each is converted to a string before the next is evaluated.
    pub(super) fn lower_template(&self, t: TemplateLiteral<'a>) -> Expression<'a> {
        let quasi = |q: &TemplateElement<'a>| {
            let cooked = q.value.cooked.map_or("", |c| c.as_str());
            self.string(q.span, cooked, q.lone_surrogates)
        };
        let mut acc = quasi(&t.quasis[0]);
        for (i, e) in t.expressions.into_iter().enumerate() {
            let mut args = vec![e];
            let next = &t.quasis[i + 1];
            if next.value.cooked.is_some_and(|c| !c.is_empty()) {
                args.push(quasi(next));
            }
            let callee = self.member(acc, "concat");
            acc = self.call(t.span, callee, args);
        }
        acc
    }

    pub(super) fn lower_tagged(
        &mut self,
        t: ArenaBox<'a, TaggedTemplateExpression<'a>>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let TaggedTemplateExpression {
            span, tag, quasi, ..
        } = t.unbox();
        let template = self.helper(Helper::Template);
        let cache = self.fresh("_t");
        self.caches.push(cache);
        let mut differ = false;
        let mut cooked = Vec::new();
        let mut raw = Vec::new();
        for q in &quasi.quasis {
            let r = q
                .value
                .raw
                .as_str()
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            match q.value.cooked {
                Some(c) => {
                    differ |= c.as_str() != r;
                    cooked.push(self.string(q.span, c.as_str(), q.lone_surrogates));
                }
                None => {
                    differ = true;
                    cooked.push(self.void0(q.span));
                }
            }
            raw.push(self.string(q.span, &r, false));
        }
        let array = |items: Vec<Expression<'a>>| {
            Expression::new_array_expression(
                SPAN,
                ArenaVec::from_iter_in(items.into_iter().map(ArrayExpressionElement::from), &ast),
                &ast,
            )
        };
        let mut args = vec![array(cooked)];
        if differ {
            args.push(array(raw));
        }
        let make = self.call(SPAN, self.ident(SPAN, template), args);
        let cached = Expression::new_logical_expression(
            SPAN,
            self.ident(SPAN, cache),
            LogicalOperator::Or,
            self.assign(cache, make),
            &ast,
        );
        let mut call_args = vec![cached];
        call_args.extend(quasi.expressions);
        self.call(span, tag, call_args)
    }

    /// The elements of an array literal or argument list with spreads, as one array:
    /// `[a].concat(__toArray(b), [c])`.
    pub(super) fn spread_array(
        &mut self,
        span: Span,
        items: Vec<(bool, Option<Expression<'a>>)>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let to_array = self.helper(Helper::ToArray);
        let mut chunks: Vec<Expression<'a>> = Vec::new();
        let mut run: Vec<ArrayExpressionElement<'a>> = Vec::new();
        let flush = |run: &mut Vec<ArrayExpressionElement<'a>>,
                     chunks: &mut Vec<Expression<'a>>| {
            if !run.is_empty() {
                chunks.push(Expression::new_array_expression(
                    SPAN,
                    ArenaVec::from_iter_in(run.drain(..), &ast),
                    &ast,
                ));
            }
        };
        for (spread, item) in items {
            match (spread, item) {
                (true, Some(e)) => {
                    flush(&mut run, &mut chunks);
                    chunks.push(self.call(e.span(), self.ident(SPAN, to_array), vec![e]));
                }
                (_, Some(e)) => run.push(ArrayExpressionElement::from(e)),
                (_, None) => run.push(ArrayExpressionElement::new_elision(SPAN, &ast)),
            }
        }
        flush(&mut run, &mut chunks);
        let mut chunks = chunks.into_iter();
        let first = chunks.next().unwrap_or_else(|| {
            Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast)
        });
        let rest: Vec<Expression<'a>> = chunks.collect();
        if rest.is_empty() && !matches!(first, Expression::ArrayExpression(_)) {
            // `[...a]` alone: a copy, as spreading makes one.
            let empty = Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast);
            return self.call(span, self.member(empty, "concat"), vec![first]);
        }
        if rest.is_empty() {
            return first;
        }
        let first = if matches!(first, Expression::ArrayExpression(_)) {
            first
        } else {
            let empty = Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast);
            return {
                let mut all = vec![first];
                all.extend(rest);
                self.call(span, self.member(empty, "concat"), all)
            };
        };
        self.call(span, self.member(first, "concat"), rest)
    }

    pub(super) fn lower_spread_call(&mut self, expr: &mut Expression<'a>) {
        let ast = self.ast();
        let Expression::CallExpression(call) = expr else {
            return;
        };
        let span = call.span;
        let items: Vec<(bool, Option<Expression<'a>>)> = call
            .arguments
            .drain(..)
            .map(|a| match a {
                Argument::SpreadElement(s) => (true, Some(s.unbox().argument)),
                a => (false, Some(a.into_expression())),
            })
            .collect();
        let args = self.spread_array(span, items);
        let callee = call.callee.take_in(&ast);
        let (callee, this) = match callee {
            Expression::StaticMemberExpression(m) => {
                let mut m = m.unbox();
                let (object, this) = self.reusable(m.object);
                m.object = object;
                (
                    Expression::StaticMemberExpression(ArenaBox::new_in(m, &ast)),
                    this,
                )
            }
            Expression::ComputedMemberExpression(m) => {
                let mut m = m.unbox();
                let (object, this) = self.reusable(m.object);
                m.object = object;
                (
                    Expression::ComputedMemberExpression(ArenaBox::new_in(m, &ast)),
                    this,
                )
            }
            Expression::Super(s) => {
                self.fail(
                    s.span.start,
                    "lower_to_es5: spread arguments to super()".to_owned(),
                );
                (Expression::Super(s), self.void0(SPAN))
            }
            callee => (callee, self.void0(SPAN)),
        };
        *expr = self.call(span, self.member(callee, "apply"), vec![this, args]);
    }

    /// `object` to use twice: itself when it has no side effects, else `(_r = object)` and `_r`.
    pub(super) fn reusable(&mut self, object: Expression<'a>) -> (Expression<'a>, Expression<'a>) {
        match &object {
            Expression::Identifier(id) => {
                let again = self.ident(SPAN, id.name);
                (object, again)
            }
            Expression::ThisExpression(_) => {
                (object, Expression::new_this_expression(SPAN, &self.ast()))
            }
            _ => {
                let temp = self.temp("_r");
                (self.assign(temp, object), self.ident(SPAN, temp))
            }
        }
    }

    pub(super) fn lower_spread_new(&mut self, expr: &mut Expression<'a>) {
        let ast = self.ast();
        let Expression::NewExpression(new) = expr else {
            return;
        };
        let span = new.span;
        let mut items = vec![(false, Some(Expression::new_null_literal(SPAN, &ast)))];
        items.extend(new.arguments.drain(..).map(|a| match a {
            Argument::SpreadElement(s) => (true, Some(s.unbox().argument)),
            a => (false, Some(a.into_expression())),
        }));
        let args = self.spread_array(span, items);
        let callee = new.callee.take_in(&ast);
        let bind = self.member(
            self.member(self.member(self.global("Function"), "prototype"), "bind"),
            "apply",
        );
        let bound = self.call(span, bind, vec![callee, args]);
        *expr = Expression::new_new_expression(span, bound, None, ArenaVec::new_in(&ast), &ast);
    }

    pub(super) fn lower_regexp(&self, r: &RegExpLiteral<'a>) -> Option<Expression<'a>> {
        let flags = r.regex.flags.to_inline_string();
        let pattern = r.regex.pattern.text.as_str();
        let unicode = flags.contains('u') || flags.contains('v');
        if flags.chars().all(|c| matches!(c, 'g' | 'i' | 'm'))
            && !pattern_is_es2018(pattern, unicode)
        {
            return None;
        }
        let ast = self.ast();
        let mut args = vec![self.string(r.span, pattern, false)];
        if !flags.is_empty() {
            args.push(self.string(SPAN, flags.as_str(), false));
        }
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &ast);
        Some(Expression::new_new_expression(
            r.span,
            self.global("RegExp"),
            None,
            args,
            &ast,
        ))
    }

    pub(super) fn lower_bigint(&self, b: &BigIntLiteral<'a>) -> Expression<'a> {
        let ast = self.ast();
        let raw = b.raw.map_or(b.value.as_str(), |r| r.as_str());
        let digits: String = raw
            .trim_end_matches('n')
            .chars()
            .filter(|&c| c != '_')
            .collect();
        let args =
            ArenaVec::from_array_in([Argument::from(self.string(SPAN, &digits, false))], &ast);
        Expression::new_call_expression_with_pure(
            b.span,
            self.global("BigInt"),
            None,
            args,
            false,
            true,
            &ast,
        )
    }

    pub(super) fn lower_import(
        &mut self,
        imp: ArenaBox<'a, ImportExpression<'a>>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let imp = imp.unbox();
        // As esbuild: options without side effects are dropped, others cannot be kept in ES5.
        if let Some(options) = &imp.options
            && !side_effect_free(options)
        {
            self.fail(
                options.span().start,
                format!(
                    "Using an arbitrary value as the second argument to \"import()\" is not possible in {}",
                    super::super::check::WHERE
                ),
            );
        }
        let require = self.helper(Helper::Require);
        let to_esm = self.helper(Helper::ToEsm);
        let required = self.call(SPAN, self.ident(SPAN, require), vec![imp.source]);
        let module = self.call(SPAN, self.ident(SPAN, to_esm), vec![required]);
        let ret = Statement::new_return_statement(SPAN, Some(module), &ast);
        let body = ArenaBox::new_in(
            FunctionBody::new(
                SPAN,
                ArenaVec::new_in(&ast),
                ArenaVec::from_array_in([ret], &ast),
                &ast,
            ),
            &ast,
        );
        let params = ArenaBox::new_in(
            FormalParameters::new(
                SPAN,
                FormalParameterKind::FormalParameter,
                ArenaVec::new_in(&ast),
                None,
                &ast,
            ),
            &ast,
        );
        let then_fn = self.function_expr(SPAN, params, body);
        let resolved = self.call(SPAN, self.member(self.global("Promise"), "resolve"), vec![]);
        self.call(imp.span, self.member(resolved, "then"), vec![then_fn])
    }
}
