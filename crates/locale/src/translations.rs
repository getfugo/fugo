//! Translation bundles and lookup.

use std::collections::BTreeMap;
use std::path::Path;

use ssg_base::{IdVec, LangIdx, Value};

use crate::error::{I18nError, MessageProblem, TranslateError};
use crate::files::{MessageFile, MessageSource};
use crate::message::Template;
use crate::plural::{PluralCount, PluralForm, PluralRules};
use crate::tag;

/// A message, parsed: one template per plural form it has.
#[derive(Debug)]
struct Message {
    forms: BTreeMap<PluralForm, Template>,
}

/// Collects the i18n files of a site, in increasing precedence (themes first, the project
/// last): a message replaces an earlier one with the same key and language.
#[derive(Debug)]
pub struct TranslationsBuilder {
    default_lang: String,
    placeholders: bool,
    bundles: BTreeMap<String, BTreeMap<String, Message>>,
}

impl TranslationsBuilder {
    /// A builder for a site whose default content language is `default_language`.
    #[must_use]
    pub fn new(default_language: &str) -> Self {
        Self {
            default_lang: tag::normalize(default_language),
            placeholders: false,
            bundles: BTreeMap::new(),
        }
    }

    /// `enableMissingTranslationPlaceholders`: a missing translation renders as
    /// `[i18n] key` instead of the default language's text or nothing.
    #[must_use]
    pub fn missing_placeholders(mut self, on: bool) -> Self {
        self.placeholders = on;
        self
    }

    /// Reads and adds the i18n file `path` (content `content`).
    ///
    /// # Errors
    /// See [`MessageFile::read`]; also a message with unsupported syntax (the error names the
    /// file, the key and the plural form).
    pub fn add_file(&mut self, path: &Path, content: &str) -> Result<(), I18nError> {
        let file = MessageFile::read(path, content)?;
        self.add(path, file)
    }

    /// Adds the messages of an already read file (`path` is used in errors).
    ///
    /// # Errors
    /// A message with unsupported syntax.
    pub fn add(&mut self, path: &Path, file: MessageFile) -> Result<(), I18nError> {
        let bundle = self.bundles.entry(file.lang).or_default();
        for source in file.messages {
            let message = compile(&source).map_err(|problem| I18nError::Message {
                path: path.to_owned(),
                key: source.id.clone(),
                problem,
            })?;
            bundle.insert(source.id, message);
        }
        Ok(())
    }

    /// The translations for the site languages `languages`, in [`LangIdx`] order.
    #[must_use]
    pub fn build<'a>(self, languages: impl IntoIterator<Item = &'a str>) -> Translations {
        let keys: Vec<String> = self.bundles.keys().cloned().collect();
        let find = |key: &str| {
            std::iter::once(key)
                .chain(tag::parents(key))
                .filter_map(|k| keys.iter().position(|b| b == k))
                .collect::<Vec<_>>()
        };
        let default_chain = find(&self.default_lang);
        let langs = languages
            .into_iter()
            .map(|lang| {
                let mut own = find(&tag::normalize(lang));
                if own.is_empty() {
                    // A language without files translates like the default language.
                    own.clone_from(&default_chain);
                }
                let fallback = default_chain
                    .iter()
                    .copied()
                    .filter(|b| !own.contains(b))
                    .collect();
                Chain { own, fallback }
            })
            .collect();
        let bundles = self
            .bundles
            .into_iter()
            .map(|(key, messages)| Bundle {
                rules: PluralRules::for_language(&key),
                messages,
            })
            .collect();
        Translations {
            bundles,
            langs,
            placeholders: self.placeholders,
        }
    }
}

fn compile(source: &MessageSource) -> Result<Message, MessageProblem> {
    let (left, right) = source
        .delims
        .as_ref()
        .map_or(("{{", "}}"), |(l, r)| (l.as_str(), r.as_str()));
    let forms = source
        .forms
        .iter()
        .filter(|(_, text)| !text.is_empty())
        .map(|(&form, text)| {
            Template::parse_with(text, left, right)
                .map(|t| (form, t))
                .map_err(|source| MessageProblem::Syntax { form, source })
        })
        .collect::<Result<_, _>>()?;
    Ok(Message { forms })
}

#[derive(Debug)]
struct Bundle {
    rules: PluralRules,
    messages: BTreeMap<String, Message>,
}

/// Where a language looks for messages: its own bundles (most specific first), then the
/// default language's.
#[derive(Debug)]
struct Chain {
    own: Vec<usize>,
    fallback: Vec<usize>,
}

/// The i18n messages of a site, per language.
#[derive(Debug)]
pub struct Translations {
    bundles: Vec<Bundle>,
    langs: IdVec<LangIdx, Chain>,
    placeholders: bool,
}

/// The arguments of a translation: the plural count and the data for the placeholders.
#[derive(Clone, Debug, Default)]
pub struct Args<'a> {
    /// Selects the plural form; without it the form is `other`.
    pub count: Option<PluralCount>,
    /// The value of `{{ . }}`; `{{ .Count }}` falls back to `count`.
    pub data: Option<&'a Value>,
}

impl<'a> Args<'a> {
    /// The Go implementation's single template argument: it is the data, and its count
    /// ([`PluralCount::from_value`]) selects the plural form.
    #[must_use]
    pub fn from_value(arg: &'a Value) -> Self {
        Self {
            count: PluralCount::from_value(arg),
            data: Some(arg),
        }
    }
}

/// The result of a lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Translation {
    /// The message of the language (or of a parent language, `pt` for `pt-br`).
    Found(String),
    /// The language has no such message; this is the default language's.
    Fallback(String),
    /// No language has the key.
    Missing,
}

impl Translations {
    /// Translations without any message.
    #[must_use]
    pub fn empty(languages: usize) -> Self {
        Self {
            bundles: Vec::new(),
            langs: (0..languages)
                .map(|_| Chain {
                    own: Vec::new(),
                    fallback: Vec::new(),
                })
                .collect(),
            placeholders: false,
        }
    }

    /// Looks `key` up for language `lang` and renders it with `args`. The plural form comes
    /// from the rules of the language whose message is used.
    ///
    /// # Errors
    /// The message lacks both the selected form and `other`, or `args` do not fit its
    /// placeholders.
    pub fn lookup(
        &self,
        lang: LangIdx,
        key: &str,
        args: &Args<'_>,
    ) -> Result<Translation, TranslateError> {
        let chain = &self.langs[lang];
        let find = |bundles: &[usize]| {
            bundles.iter().find_map(|&b| {
                let bundle = &self.bundles[b];
                bundle.messages.get(key).map(|m| (bundle, m))
            })
        };
        if let Some((bundle, message)) = find(&chain.own) {
            return render(bundle, message, key, args).map(Translation::Found);
        }
        if let Some((bundle, message)) = find(&chain.fallback) {
            return render(bundle, message, key, args).map(Translation::Fallback);
        }
        Ok(Translation::Missing)
    }

    /// The Go implementation's `i18n`: the translation, the default language's text when the
    /// language lacks the key, or nothing; with missing-translation placeholders, `[i18n] key`
    /// for both of the latter.
    ///
    /// # Errors
    /// As [`lookup`](Self::lookup).
    pub fn translate(
        &self,
        lang: LangIdx,
        key: &str,
        args: &Args<'_>,
    ) -> Result<String, TranslateError> {
        Ok(match self.lookup(lang, key, args)? {
            Translation::Found(s) => s,
            Translation::Fallback(_) | Translation::Missing if self.placeholders => {
                format!("[i18n] {key}")
            }
            Translation::Fallback(s) => s,
            Translation::Missing => String::new(),
        })
    }
}

fn render(
    bundle: &Bundle,
    message: &Message,
    key: &str,
    args: &Args<'_>,
) -> Result<String, TranslateError> {
    let form = args
        .count
        .as_ref()
        .map_or(PluralForm::Other, |c| bundle.rules.form(c));
    let template = message
        .forms
        .get(&form)
        .or_else(|| message.forms.get(&PluralForm::Other))
        .ok_or_else(|| TranslateError::MissingForm {
            key: key.to_owned(),
            form,
        })?;
    let data = args.data.unwrap_or(&Value::Null);
    template
        .render(data, args.count.as_ref())
        .map_err(|source| TranslateError::Eval {
            key: key.to_owned(),
            source,
        })
}
