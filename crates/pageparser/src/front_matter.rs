//! Front matter: splitting it off a content file and decoding it into folded [`Params`].

use ssg_base::{Map, Params, Value};

use crate::lexer::{LexError, lex_intro};
use crate::token::FrontMatterFormat;

/// A content file split into front matter and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split<'a> {
    /// The format and text of the front matter (YAML and TOML without their delimiter lines).
    pub front_matter: Option<(FrontMatterFormat, &'a str)>,
    /// The content after the front matter (and after any byte order mark).
    pub body: &'a str,
    /// The byte offset of `body` in the file.
    pub body_offset: usize,
}

/// Splits the front matter off a content file. Leading blank lines and byte order marks are
/// skipped; delimiter lines may end in CRLF.
///
/// # Errors
/// A page that starts with `-`, `+` or `{` but has no well-formed front matter (Go reads
/// those as front matter too).
pub fn split_front_matter(src: &str) -> Result<Split<'_>, LexError> {
    let intro = lex_intro(src.as_bytes())?;
    let front_matter = intro.front_matter.map(|(f, span)| (f, &src[span]));
    Ok(Split {
        front_matter,
        body: &src[intro.body_offset..],
        body_offset: intro.body_offset,
    })
}

/// Front matter that could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid {format} front matter: {source}")]
    Syntax {
        format: FrontMatterFormat,
        source: ssg_base::value::DecodeError,
    },
    #[error("{format} front matter is not a map")]
    NotAMap { format: FrontMatterFormat },
    #[error("Org front matter is not supported")]
    Unsupported,
}

/// Decodes front matter into a case-preserving map (an empty or `null` document is an empty
/// map). YAML is YAML 1.2 (`yes` is a string) with timestamps kept as strings; TOML dates
/// are [`ssg_base::Date`]s; JSON integers stay integers.
///
/// # Errors
/// Invalid syntax, a document that is not a map, or Org front matter.
pub fn decode_front_matter_map(format: FrontMatterFormat, text: &str) -> Result<Map, DecodeError> {
    let decoded = match format {
        FrontMatterFormat::Yaml if text.trim().is_empty() => Ok(Value::Null),
        FrontMatterFormat::Yaml => Value::from_yaml_str(text),
        FrontMatterFormat::Toml => Value::from_toml_str(text),
        FrontMatterFormat::Json => Value::from_json_str(text),
        FrontMatterFormat::Org => return Err(DecodeError::Unsupported),
    }
    .map_err(|source| DecodeError::Syntax { format, source })?;
    match decoded {
        Value::Null => Ok(Map::new()),
        Value::Map(m) => Ok(std::sync::Arc::unwrap_or_clone(m)),
        _ => Err(DecodeError::NotAMap { format }),
    }
}

/// Decodes front matter into [`Params`] (keys lower-cased recursively, not inside arrays).
///
/// # Errors
/// As [`decode_front_matter_map`].
pub fn decode_front_matter(format: FrontMatterFormat, text: &str) -> Result<Params, DecodeError> {
    decode_front_matter_map(format, text).map(|m| Params::fold(&m))
}
