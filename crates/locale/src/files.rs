//! Reading i18n files: TOML, YAML or JSON, in the Go implementation's message layouts.
//!
//! - flat: `key = "text"`;
//! - one message per table: `[key]` with `one = …`, `other = …` (and `zero`, `two`, `few`,
//!   `many`, `description`, `hash`, `leftDelim`, `rightDelim`, `id`);
//! - nested namespaces: `[ns.key]` or `ns: {key: …}` give the key `ns.key`;
//! - the list layout: `[{id = "key", translation = "text" | {one = …, other = …}}]`.
//!
//! A map is a message when one of the reserved keys above (written in lower case) has a string
//! value; otherwise it is a namespace.

use std::collections::BTreeMap;
use std::path::Path;

use ssg_base::Value;

use crate::error::{I18nError, MessageProblem};
use crate::plural::PluralForm;
use crate::tag;

/// One message as written in a file: its key, delimiters and the text of each plural form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageSource {
    /// The message key (`readingTime`, `nav.home`); case-sensitive.
    pub id: String,
    /// Custom delimiters (`leftDelim`/`rightDelim`), when set.
    pub delims: Option<(String, String)>,
    /// Plural form → text. An empty text counts as absent.
    pub forms: BTreeMap<PluralForm, String>,
}

/// The messages of one i18n file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageFile {
    /// The language key from the file name (`en`, `pt-br`), normalised.
    pub lang: String,
    /// The messages, in file order for lists and key order for maps.
    pub messages: Vec<MessageSource>,
}

const MESSAGE_KEYS: [&str; 11] = [
    "id",
    "description",
    "hash",
    "leftdelim",
    "rightdelim",
    "zero",
    "one",
    "two",
    "few",
    "many",
    "other",
];

impl MessageFile {
    /// Reads the i18n file `path` with content `content`. The language comes from the file
    /// name (`i18n/pt-BR.yaml` → `pt-br`), the format from the extension.
    ///
    /// # Errors
    /// An unknown extension, a file name that is not a language code, a file that does not
    /// parse, or a malformed message.
    pub fn read(path: &Path, content: &str) -> Result<Self, I18nError> {
        let lang = lang_of(path).ok_or_else(|| I18nError::FileName {
            path: path.to_owned(),
        })?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let decoded = match ext.as_deref() {
            Some("toml") => Value::from_toml_str(content),
            Some("yaml" | "yml") => Value::from_yaml_str(content),
            Some("json") => Value::from_json_str(content),
            _ => {
                return Err(I18nError::Format {
                    path: path.to_owned(),
                });
            }
        };
        let root = decoded.map_err(|source| I18nError::Decode {
            path: path.to_owned(),
            source: Box::new(source),
        })?;
        let mut reader = Reader {
            path,
            messages: Vec::new(),
        };
        match &root {
            Value::Null => {}
            Value::Map(_) | Value::Array(_) => reader.entry("", &root)?,
            other => {
                return Err(I18nError::NotMessages {
                    path: path.to_owned(),
                    found: kind(other),
                });
            }
        }
        Ok(Self {
            lang,
            messages: reader.messages,
        })
    }
}

/// The language key of an i18n file: its name without the extension, if that is made of
/// letters, digits, `-` and `_`.
fn lang_of(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let valid = !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| tag::normalize(stem))
}

struct Reader<'a> {
    path: &'a Path,
    messages: Vec<MessageSource>,
}

impl Reader<'_> {
    fn error(&self, key: &str, problem: MessageProblem) -> I18nError {
        I18nError::Message {
            path: self.path.to_owned(),
            key: key.to_owned(),
            problem,
        }
    }

    /// A namespace entry at `id` (the key path so far).
    fn entry(&mut self, id: &str, v: &Value) -> Result<(), I18nError> {
        match v {
            Value::String(s) => {
                let mut m = MessageSource {
                    id: id.to_owned(),
                    ..MessageSource::default()
                };
                m.forms.insert(PluralForm::Other, s.to_string());
                self.messages.push(m);
            }
            Value::Map(map) if is_message(map) => self.message(id, map)?,
            Value::Map(map) => {
                for (k, child) in map.iter() {
                    let child_id = if id.is_empty() {
                        k.to_owned()
                    } else {
                        format!("{id}.{k}")
                    };
                    self.entry(&child_id, child)?;
                }
            }
            Value::Array(items) => {
                for item in items.iter() {
                    match item {
                        Value::Map(m) if m.contains_key("translation") || m.contains_key("id") => {
                            self.list_message(id, m)?;
                        }
                        item => self.entry(id, item)?,
                    }
                }
            }
            other => {
                return Err(self.error(id, MessageProblem::NotAMessage { found: kind(other) }));
            }
        }
        Ok(())
    }

    /// A message map at `id`.
    fn message(&mut self, id: &str, map: &ssg_base::Map) -> Result<(), I18nError> {
        let mut m = MessageSource {
            id: id.to_owned(),
            ..MessageSource::default()
        };
        let mut left = None;
        let mut right = None;
        for (k, v) in map.iter() {
            let field = k.to_ascii_lowercase();
            let text = match v {
                Value::Null => continue,
                Value::String(s) => s.to_string(),
                Value::Map(forms) if field == "translation" => {
                    self.forms(&m.id, forms, &mut m.forms)?;
                    continue;
                }
                // Unknown keys are ignored when they are strings, an error otherwise.
                v => {
                    return Err(self.error(
                        id,
                        MessageProblem::NotAString {
                            field: k.to_owned(),
                            found: kind(v),
                        },
                    ));
                }
            };
            match field.as_str() {
                "id" => m.id = text,
                "leftdelim" => left = Some(text),
                "rightdelim" => right = Some(text),
                "translation" => {
                    m.forms.insert(PluralForm::Other, text);
                }
                f => {
                    if let Some(form) = PluralForm::from_keyword(f) {
                        m.forms.insert(form, text);
                    }
                }
            }
        }
        m.delims = left.zip(right);
        self.messages.push(m);
        Ok(())
    }

    /// An element of the list layout: `{id, translation}` under the namespace `prefix`.
    fn list_message(&mut self, prefix: &str, map: &ssg_base::Map) -> Result<(), I18nError> {
        let id = match map.get("id") {
            Some(Value::String(s)) if prefix.is_empty() => s.to_string(),
            Some(Value::String(s)) => format!("{prefix}.{s}"),
            _ => prefix.to_owned(),
        };
        let mut m = MessageSource {
            id,
            ..MessageSource::default()
        };
        match map.get("translation") {
            Some(Value::String(s)) => {
                m.forms.insert(PluralForm::Other, s.to_string());
            }
            Some(Value::Map(forms)) => self.forms(&m.id, forms, &mut m.forms)?,
            Some(other) => {
                return Err(self.error(
                    &m.id,
                    MessageProblem::NotAString {
                        field: "translation".to_owned(),
                        found: kind(other),
                    },
                ));
            }
            None => {}
        }
        self.messages.push(m);
        Ok(())
    }

    /// The plural forms of a `translation` map.
    fn forms(
        &self,
        id: &str,
        map: &ssg_base::Map,
        out: &mut BTreeMap<PluralForm, String>,
    ) -> Result<(), I18nError> {
        for (k, v) in map.iter() {
            match v {
                Value::Null => {}
                Value::String(s) => {
                    if let Some(form) = PluralForm::from_keyword(&k.to_ascii_lowercase()) {
                        out.insert(form, s.to_string());
                    }
                }
                v => {
                    return Err(self.error(
                        id,
                        MessageProblem::NotAString {
                            field: k.to_owned(),
                            found: kind(v),
                        },
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Whether a map is a message: a reserved key (exact spelling) with a string value.
fn is_message(map: &ssg_base::Map) -> bool {
    map.iter().any(|(k, v)| {
        matches!(v, Value::String(_))
            && (MESSAGE_KEYS.contains(&k) || k == "leftDelim" || k == "rightDelim")
    })
}

pub(crate) fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Int(_) => "integer",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Date(_) => "date",
        Value::Array(_) => "list",
        Value::Map(_) => "map",
    }
}
