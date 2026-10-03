//! The self-check of [`super::lower_to_es5`]: no ES2015+ syntax may remain.
//!
//! The first offending node is an error, named with esbuild's wording where esbuild names the
//! feature, at its position in the input bundle (nodes the lowering created have none: 0).

use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::span::GetSpan;
use oxc::syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator};
use oxc::syntax::scope::ScopeFlags;

use super::check::WHERE;
use super::lower::{Error, pattern_is_es2018};

pub(super) fn verify(program: &Program<'_>) -> Result<(), Error> {
    let mut v = Verifier { error: None };
    v.visit_program(program);
    v.error.map_or(Ok(()), Err)
}

struct Verifier {
    error: Option<Error>,
}

impl Verifier {
    fn found(&mut self, pos: u32, what: &str) {
        if self.error.is_none() {
            self.error = Some((
                pos,
                format!("Transforming {what} to {WHERE} is not supported yet"),
            ));
        }
    }

    fn found_message(&mut self, pos: u32, message: String) {
        if self.error.is_none() {
            self.error = Some((pos, message));
        }
    }

    fn module_name(&mut self, name: &ModuleExportName<'_>, kind: &str) {
        if let ModuleExportName::StringLiteral(s) = name {
            self.found_message(
                s.span.start,
                format!(
                    "Using the string \"{}\" as an {kind} name is not supported in {WHERE}",
                    s.value
                ),
            );
        }
    }
}

impl<'a> Visit<'a> for Verifier {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        match it.kind {
            VariableDeclarationKind::Var => {}
            VariableDeclarationKind::Let => self.found(it.span.start, "let"),
            VariableDeclarationKind::Const => self.found(it.span.start, "const"),
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => {
                self.found(it.span.start, "using declarations");
            }
        }
        walk::walk_variable_declaration(self, it);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.found(it.span.start, "arrow functions");
        walk::walk_arrow_function_expression(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        self.found(it.span.start, "class syntax");
        walk::walk_class(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        match (it.r#async, it.generator) {
            (true, true) => self.found(it.span.start, "async generator functions"),
            (true, false) => self.found(it.span.start, "async functions"),
            (false, true) => self.found(it.span.start, "generator functions"),
            (false, false) => {}
        }
        walk::walk_function(self, it, flags);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if let Some(init) = &it.initializer {
            self.found(init.span().start, "default arguments");
        }
        walk::walk_formal_parameter(self, it);
    }

    fn visit_formal_parameter_rest(&mut self, it: &FormalParameterRest<'a>) {
        self.found(it.span.start, "rest arguments");
        walk::walk_formal_parameter_rest(self, it);
    }

    fn visit_binding_pattern(&mut self, it: &BindingPattern<'a>) {
        if !matches!(it, BindingPattern::BindingIdentifier(_)) {
            self.found(it.span().start, "destructuring");
        }
        walk::walk_binding_pattern(self, it);
    }

    fn visit_assignment_target_pattern(&mut self, it: &AssignmentTargetPattern<'a>) {
        self.found(it.span().start, "destructuring");
        walk::walk_assignment_target_pattern(self, it);
    }

    fn visit_array_expression_element(&mut self, it: &ArrayExpressionElement<'a>) {
        if let ArrayExpressionElement::SpreadElement(s) = it {
            self.found(s.span.start, "array spread");
        }
        walk::walk_array_expression_element(self, it);
    }

    fn visit_argument(&mut self, it: &Argument<'a>) {
        if let Argument::SpreadElement(s) = it {
            self.found(s.span.start, "rest arguments");
        }
        walk::walk_argument(self, it);
    }

    fn visit_object_property_kind(&mut self, it: &ObjectPropertyKind<'a>) {
        match it {
            ObjectPropertyKind::SpreadProperty(s) => self.found(s.span.start, "object spread"),
            ObjectPropertyKind::ObjectProperty(p) => {
                let printed_shorthand = matches!(
                    (&p.key, &p.value),
                    (PropertyKey::StaticIdentifier(k), Expression::Identifier(v)) if k.name == v.name
                );
                let bracketed_number = matches!(&p.key, PropertyKey::NumericLiteral(n) if !n.value.is_finite() || n.value < 0.0);
                if p.shorthand || p.method || p.computed || printed_shorthand || bracketed_number {
                    self.found(p.span.start, "object literal extensions");
                }
            }
        }
        walk::walk_object_property_kind(self, it);
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.found(it.span.start, "template literals");
        walk::walk_template_literal(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.found(it.span.start, "tagged template literals");
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.found(it.span.start, "for-of loops");
        walk::walk_for_of_statement(self, it);
    }

    fn visit_yield_expression(&mut self, it: &YieldExpression<'a>) {
        self.found(it.span.start, "generator functions");
        walk::walk_yield_expression(self, it);
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        self.found(it.span.start, "async functions");
        walk::walk_await_expression(self, it);
    }

    fn visit_new_target(&mut self, it: &NewTarget) {
        self.found(it.span.start, "new.target");
    }

    fn visit_import_meta(&mut self, it: &ImportMeta) {
        self.found(it.span.start, "import.meta");
    }

    fn visit_super(&mut self, it: &Super) {
        self.found(it.span.start, "super");
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.found(it.span.start, "dynamic import()");
        walk::walk_import_expression(self, it);
    }

    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        self.found(it.span.start, "optional chaining");
        walk::walk_chain_expression(self, it);
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        if it.operator == LogicalOperator::Coalesce {
            self.found(it.span.start, "nullish coalescing");
        }
        walk::walk_logical_expression(self, it);
    }

    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        if it.operator == BinaryOperator::Exponential {
            self.found(it.span.start, "the exponent operator");
        }
        walk::walk_binary_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        match it.operator {
            AssignmentOperator::Exponential => self.found(it.span.start, "the exponent operator"),
            AssignmentOperator::LogicalAnd
            | AssignmentOperator::LogicalOr
            | AssignmentOperator::LogicalNullish => self.found(it.span.start, "logical assignment"),
            _ => {}
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_big_int_literal(&mut self, it: &BigIntLiteral<'a>) {
        self.found(it.span.start, "big integer literals");
    }

    fn visit_reg_exp_literal(&mut self, it: &RegExpLiteral<'a>) {
        let flags = it.regex.flags.to_inline_string();
        let unicode = flags.contains('u') || flags.contains('v');
        if !flags.chars().all(|c| matches!(c, 'g' | 'i' | 'm'))
            || pattern_is_es2018(it.regex.pattern.text.as_str(), unicode)
        {
            self.found(it.span.start, "regular expression syntax");
        }
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        if it.param.is_none() {
            self.found(it.span.start, "optional catch bindings");
        }
        walk::walk_catch_clause(self, it);
    }

    fn visit_private_identifier(&mut self, it: &PrivateIdentifier<'a>) {
        self.found(it.span.start, "class syntax");
    }

    fn visit_private_in_expression(&mut self, it: &PrivateInExpression<'a>) {
        self.found(it.span.start, "class syntax");
        walk::walk_private_in_expression(self, it);
    }

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        self.found(it.span.start, "decorators");
    }

    fn visit_export_specifier(&mut self, it: &ExportSpecifier<'a>) {
        self.module_name(&it.exported, "export");
        self.module_name(&it.local, "export");
        walk::walk_export_specifier(self, it);
    }

    fn visit_import_specifier(&mut self, it: &ImportSpecifier<'a>) {
        self.module_name(&it.imported, "import");
        walk::walk_import_specifier(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        if let Some(exported) = &it.exported {
            self.module_name(exported, "export");
            self.found(it.span.start, "export star as");
        }
        walk::walk_export_all_declaration(self, it);
    }

    fn visit_with_clause(&mut self, it: &WithClause<'a>) {
        self.found(it.span.start, "import attributes");
    }

    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        self.found(it.span.start, "JSX");
    }

    fn visit_jsx_fragment(&mut self, it: &JSXFragment<'a>) {
        self.found(it.span.start, "JSX");
    }
}
