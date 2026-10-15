//! The texts written into published images: a copyright notice and an author, `[imaging]`'s or
//! an image's own (a page resource's `copyright` and `author` params), and what they may hold.

use super::*;

/// Why a text cannot be written into images as a copyright notice or an author.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NoticeError {
    /// More than [`ImagingConfig::NOTICE_MAX`] bytes, or a control character (a line break).
    #[error(
        "must be at most {} bytes of text without control characters",
        ImagingConfig::NOTICE_MAX
    )]
    Text,
    /// A brace of a copyright notice outside [`ImagingConfig::YEAR`], with what it encloses:
    /// `{yaer}`, or the lone `{` or `}`.
    #[error("{0:?} is not a placeholder: the only one is {year}", year = ImagingConfig::YEAR)]
    Placeholder(String),
}

impl ImagingConfig {
    /// The longest copyright notice or author, in bytes: they go into published images.
    pub const NOTICE_MAX: usize = 1000;

    /// `copyright` (`[imaging]`'s or an image's own) with each [`YEAR`](Self::YEAR) replaced
    /// by `year`.
    #[must_use]
    pub fn replace_year(copyright: &str, year: i16) -> String {
        copyright.replace(Self::YEAR, &year.to_string())
    }

    /// Checks a copyright notice: an author's rules ([`check_author`](Self::check_author)),
    /// and braces only in [`YEAR`](Self::YEAR), so that a typo such as `{yaer}` does not go
    /// into the images as it is.
    ///
    /// # Errors
    /// [`NoticeError`].
    pub fn check_copyright(text: &str) -> Result<(), NoticeError> {
        Self::check_author(text)?;
        match stray_brace(text) {
            Some(brace) => Err(NoticeError::Placeholder(brace.to_owned())),
            None => Ok(()),
        }
    }

    /// Checks an author: at most [`NOTICE_MAX`](Self::NOTICE_MAX) bytes, without control
    /// characters (line breaks).
    ///
    /// # Errors
    /// [`NoticeError::Text`].
    pub fn check_author(text: &str) -> Result<(), NoticeError> {
        if text.len() > Self::NOTICE_MAX || text.chars().any(char::is_control) {
            return Err(NoticeError::Text);
        }
        Ok(())
    }
}

/// The first brace of `text` outside a [`YEAR`](ImagingConfig::YEAR), with what it encloses:
/// `{yaer}`, or the lone `{` or `}`.
fn stray_brace(text: &str) -> Option<&str> {
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        let tail = &rest[i..];
        if let Some(after) = tail.strip_prefix(ImagingConfig::YEAR) {
            rest = after;
            continue;
        }
        return Some(match tail.find('}') {
            Some(end) if tail.starts_with('{') => &tail[..=end],
            _ => &tail[..1],
        });
    }
    None
}
