//! An image's own copyright notice and author: a page resource's `copyright` and `author`
//! params, written in place of `[imaging]`'s.

use ssg_base::{Params, Value};
use ssg_config::{ImagingConfig, NoticeError};

use super::ImageMetadata;

/// The copyright notice and author an image gives itself (a page resource's `copyright` and
/// `author` params), written into it and into the images processed from it in place of both of
/// `[imaging]`'s, so that a photo by someone else never carries the site's copyright next to
/// its own author. An empty one is not written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credit {
    /// The copyright notice, `{year}` replaced (empty: none).
    pub copyright: String,
    /// The author (empty: none).
    pub author: String,
}

/// Why an image's `copyright` or `author` param cannot be written into it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CreditError {
    /// A value that is not a string.
    #[error("params.{key}: expected a string, found {found}")]
    NotText {
        key: &'static str,
        found: &'static str,
    },
    /// Text that `[imaging]` refuses too.
    #[error("params.{key}: {source}")]
    Text {
        key: &'static str,
        #[source]
        source: NoticeError,
    },
}

impl ImageMetadata {
    /// The credit an image resource gives itself in `params` (a page resource's, its
    /// `resources` metadata applied): its `copyright` and `author`, `{year}` in the copyright
    /// the build's ([`year`](Self::year)). `None` when it sets neither, or when images are
    /// published as they are ([`strip`](Self::strip) off): the params are then only the site's
    /// own, and are not read.
    ///
    /// # Errors
    /// A `copyright` or `author` that is not a string, or that `[imaging]` would refuse too
    /// ([`ImagingConfig::check_copyright`], [`ImagingConfig::check_author`]).
    pub fn credit(&self, params: &Params) -> Result<Option<Credit>, CreditError> {
        let (copyright, author) = (params.get("copyright"), params.get("author"));
        if !self.strip || (copyright.is_none() && author.is_none()) {
            return Ok(None);
        }
        let copyright = text("copyright", copyright, ImagingConfig::check_copyright)?;
        let author = text("author", author, ImagingConfig::check_author)?;
        Ok(Some(Credit {
            copyright: ImagingConfig::replace_year(copyright, self.year),
            author: author.to_owned(),
        }))
    }
}

/// The text of param `key` (empty when it is not set), as `check` accepts it.
fn text<'a>(
    key: &'static str,
    value: Option<&'a Value>,
    check: fn(&str) -> Result<(), NoticeError>,
) -> Result<&'a str, CreditError> {
    let text = match value {
        None => "",
        Some(Value::String(s)) => s,
        Some(other) => {
            return Err(CreditError::NotText {
                key,
                found: kind(other),
            });
        }
    };
    check(text).map_err(|source| CreditError::Text { key, source })?;
    Ok(text)
}

/// The shape of `v`, for error messages.
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Int(_) => "an integer",
        Value::Float(_) => "a number",
        Value::String(_) => "a string",
        Value::Date(_) => "a date",
        Value::Array(_) => "a list",
        Value::Map(_) => "a table",
    }
}
