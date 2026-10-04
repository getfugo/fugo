//! The word lists of Chroma's Common Lisp and Emacs Lisp lexers (`lexers/cl.go`,
//! `lexers/emacs.go`, Chroma v2.19.0, MIT): names their XML rules emit as `NameVariable` and
//! the lexers remap by word. Each list is a file of `lisp/`, one word per line, in Chroma's order.

pub(super) const CL_BUILTIN_FUNCTIONS: &str = include_str!("lisp/cl_builtin_functions.txt");
pub(super) const CL_SPECIAL_FORMS: &str = include_str!("lisp/cl_special_forms.txt");
pub(super) const CL_MACROS: &str = include_str!("lisp/cl_macros.txt");
pub(super) const CL_LAMBDA_LIST_KEYWORDS: &str = include_str!("lisp/cl_lambda_list_keywords.txt");
pub(super) const CL_DECLARATIONS: &str = include_str!("lisp/cl_declarations.txt");
pub(super) const CL_BUILTIN_TYPES: &str = include_str!("lisp/cl_builtin_types.txt");
pub(super) const CL_BUILTIN_CLASSES: &str = include_str!("lisp/cl_builtin_classes.txt");
pub(super) const EMACS_MACROS: &str = include_str!("lisp/emacs_macros.txt");
pub(super) const EMACS_SPECIAL_FORMS: &str = include_str!("lisp/emacs_special_forms.txt");
pub(super) const EMACS_BUILTIN_FUNCTION: &str = include_str!("lisp/emacs_builtin_function.txt");
pub(super) const EMACS_BUILTIN_FUNCTION_HIGHLIGHTED: &str =
    include_str!("lisp/emacs_builtin_function_highlighted.txt");
pub(super) const EMACS_LAMBDA_LIST_KEYWORDS: &str =
    include_str!("lisp/emacs_lambda_list_keywords.txt");
pub(super) const EMACS_ERROR_KEYWORDS: &str = include_str!("lisp/emacs_error_keywords.txt");

/// The words of a list: its lines.
pub(super) fn words(list: &str) -> Vec<&str> {
    list.lines().collect()
}
