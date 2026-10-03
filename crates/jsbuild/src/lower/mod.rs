//! The source transforms rolldown does not do itself: TC39 decorators and the `es5` target.

pub(crate) mod decorators;
pub(crate) mod es5;

/// A transformed module or bundle, with its source map from the input to `code`.
#[derive(Debug)]
pub(crate) struct Lowered {
    pub(crate) code: String,
    pub(crate) map: Option<rolldown_sourcemap::SourceMap>,
}

/// An error at a position of a transform's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LowerError {
    pub(crate) message: String,
    /// 1-based.
    pub(crate) line: u32,
    /// 0-based, in bytes.
    pub(crate) column: u32,
}
