//! JavaScript through oxc: parse (script or module, detected or given), compress, mangle, print.

use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, WithStatement};
use oxc_ast_visit::Visit;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::MinifyError;
use crate::options::JsOptions;

pub(crate) fn minify(o: &JsOptions, input: &str) -> Result<String, MinifyError> {
    minify_as(o, SourceType::unambiguous(), input)
}

/// `input` parsed as `source_type`: an inline `<script>` is a classic script or a module by its
/// `type`, not by its content. Top-level names are never mangled (a classic script's are
/// globals).
pub(crate) fn minify_as(
    o: &JsOptions,
    source_type: SourceType,
    input: &str,
) -> Result<String, MinifyError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, input, source_type).parse();
    if let Some(e) = parsed.errors.first() {
        return Err(MinifyError::Js(e.to_string()));
    }
    let mut program = parsed.program;
    // A `with` body resolves names against an object at run time, which oxc's mangler and
    // compressor do not account for: such a script only loses its whitespace and comments.
    let with = has_with(&program);
    let minified = Minifier::new(MinifierOptions {
        mangle: (!o.keep_var_names && !with).then(MangleOptions::default),
        // `safest`: no transformation that relies on assumptions about the environment.
        compress: (!with).then(CompressOptions::safest),
    })
    .minify(&allocator, &mut program);
    // The mangled names live in the returned scoping and private-member maps.
    Ok(Codegen::new()
        .with_options(CodegenOptions::minify())
        .with_scoping(minified.scoping)
        .with_private_member_mappings(minified.class_private_mappings)
        .build(&program)
        .code)
}

fn has_with(program: &Program<'_>) -> bool {
    struct FindWith(bool);
    impl<'a> Visit<'a> for FindWith {
        fn visit_with_statement(&mut self, _: &WithStatement<'a>) {
            self.0 = true;
        }
    }
    let mut find = FindWith(false);
    find.visit_program(program);
    find.0
}
