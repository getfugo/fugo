//! JavaScript through oxc: parse (script or module, detected), compress, mangle, print.

use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::MinifyError;
use crate::options::JsOptions;

pub(crate) fn minify(o: &JsOptions, input: &str) -> Result<String, MinifyError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, input, SourceType::unambiguous()).parse();
    if let Some(e) = parsed.errors.first() {
        return Err(MinifyError::Js(e.to_string()));
    }
    let mut program = parsed.program;
    let minified = Minifier::new(MinifierOptions {
        mangle: (!o.keep_var_names).then(MangleOptions::default),
        // `safest`: no transformation that relies on assumptions about the environment.
        compress: Some(CompressOptions::safest()),
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
