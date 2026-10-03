//! Language-dependent behaviour, on ICU4X compiled data (CLDR, Unicode-3.0):
//!
//! - [`Locale`]: one language's collation ([`base::Collate`](ssg_base::Collate)), plural
//!   rules, number format and date names;
//! - [`Translations`]: the i18n bundles (`i18n/*.{toml,yaml,yml,json}`) with the Go
//!   implementation's message file layouts, and the restricted message evaluator: a message is
//!   text with `{{ . }}` and `{{ .Field }}` placeholders, nothing else ([`Template`]);
//! - [`format_number`] and [`format_date`] (Gregorian calendar in every language, Thai
//!   included).
//!
//! The crate README lists the accepted deviations from the Go implementation (newer CLDR, the
//! smaller message syntax).

#![forbid(unsafe_code)]

mod collate;
mod date;
mod error;
mod files;
mod locale;
mod message;
mod number;
mod plural;
mod tag;
mod translations;

pub use collate::Collator;
pub use date::{DateFormatError, DatePattern, DateStyle, NameWidth, format_date};
pub use error::{I18nError, MessageProblem, TranslateError};
pub use files::{MessageFile, MessageSource};
pub use locale::Locale;
pub use message::{EvalError, NO_VALUE, Piece, SyntaxError, Template};
pub use number::format_number;
pub use plural::{PluralCount, PluralForm, PluralRules};
pub use translations::{Args, Translation, Translations, TranslationsBuilder};
