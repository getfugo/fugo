//! Errors of loading and evaluating translations.

use std::path::PathBuf;

use ssg_base::value::DecodeError;

use crate::message::{EvalError, SyntaxError};
use crate::plural::PluralForm;

/// An i18n file that cannot be loaded. Every variant names the file; message problems also
/// name the message key.
#[derive(Debug, thiserror::Error)]
pub enum I18nError {
    /// The extension is not `.toml`, `.yaml`, `.yml` or `.json`.
    #[error("{path}: not an i18n file format (expected .toml, .yaml, .yml or .json)")]
    Format { path: PathBuf },
    /// The file name (without extension) is not a language key.
    #[error("{path}: the file name is not a language code")]
    FileName { path: PathBuf },
    /// The file does not parse.
    #[error("{path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: Box<DecodeError>,
    },
    /// The file is not a map or list of messages.
    #[error("{path}: expected a map of messages, found a {found}")]
    NotMessages { path: PathBuf, found: &'static str },
    /// A message is malformed or uses unsupported syntax.
    #[error("{path}: message `{key}`: {problem}")]
    Message {
        path: PathBuf,
        key: String,
        problem: MessageProblem,
    },
}

/// What is wrong with one message.
#[derive(Debug, thiserror::Error)]
pub enum MessageProblem {
    /// A message field that must be a string is not.
    #[error("`{field}` must be a string, found a {found}")]
    NotAString { field: String, found: &'static str },
    /// A namespace entry that is neither a message nor a namespace.
    #[error("expected a message or a map of messages, found a {found}")]
    NotAMessage { found: &'static str },
    /// The text of plural form `form` is not a supported message.
    #[error("plural form `{form}`: {source}")]
    Syntax {
        form: PluralForm,
        #[source]
        source: SyntaxError,
    },
}

/// A translation that exists but cannot be produced for the given arguments.
#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    /// The message has neither the selected plural form nor `other`.
    #[error("message `{key}` has no `{form}` form and no `other` form")]
    MissingForm { key: String, form: PluralForm },
    /// The argument does not fit the message's placeholders.
    #[error("message `{key}`: {source}")]
    Eval {
        key: String,
        #[source]
        source: EvalError,
    },
}
