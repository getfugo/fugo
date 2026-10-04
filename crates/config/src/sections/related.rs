//! Related content: the indexes and their weights.

use super::*;

/// `[related]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatedConfig {
    /// Minimum score (0–100).
    pub threshold: u8,
    pub include_newer: bool,
    pub to_lower: bool,
    pub indices: Vec<RelatedIndex>,
}

/// One index of `[[related.indices]]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatedIndex {
    /// Front matter key or taxonomy (lower case).
    pub name: String,
    pub kind: RelatedIndexKind,
    pub weight: i32,
    pub cardinality_threshold: i32,
    /// A Go-layout date pattern for date indices.
    pub pattern: String,
    pub to_lower: bool,
    pub apply_filter: bool,
}

/// What a related-content index matches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RelatedIndexKind {
    /// Front matter values.
    Basic,
    /// Heading ids (`.Fragments`).
    Fragments,
}

impl RelatedConfig {
    /// Go's default: `keywords` (100), `date` (10) and, when there is a `tag` taxonomy,
    /// `tags` (80); threshold 80.
    #[must_use]
    pub fn default_for(has_tags: bool) -> Self {
        let idx = |name: &str, weight| RelatedIndex {
            name: name.to_owned(),
            kind: RelatedIndexKind::Basic,
            weight,
            cardinality_threshold: 0,
            pattern: String::new(),
            to_lower: false,
            apply_filter: false,
        };
        let mut indices = vec![idx("keywords", 100), idx("date", 10)];
        if has_tags {
            indices.push(idx("tags", 80));
        }
        Self {
            threshold: 80,
            include_newer: false,
            to_lower: false,
            indices,
        }
    }

    pub(crate) fn decode(config: &Map) -> Result<Self, crate::de::DeError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Raw {
            threshold: i64,
            include_newer: bool,
            to_lower: bool,
            indices: Vec<RawIndex>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct RawIndex {
            name: String,
            #[serde(rename = "type")]
            kind: String,
            weight: i32,
            cardinality_threshold: i32,
            pattern: String,
            to_lower: bool,
            apply_filter: bool,
        }
        let r: Raw = crate::de::from_map(config)?;
        let threshold = u8::try_from(r.threshold)
            .ok()
            .filter(|t| *t <= 100)
            .ok_or_else(|| crate::de::DeError {
                path: vec!["threshold".to_owned()],
                message: "must be between 0 and 100".to_owned(),
            })?;
        let mut indices = Vec::with_capacity(r.indices.len());
        for (i, ix) in r.indices.into_iter().enumerate() {
            let kind = match ix.kind.to_ascii_lowercase().as_str() {
                "" | "basic" => RelatedIndexKind::Basic,
                "fragments" => RelatedIndexKind::Fragments,
                other => {
                    return Err(crate::de::DeError {
                        path: vec!["indices".to_owned(), i.to_string(), "type".to_owned()],
                        message: format!("unknown index type {other:?} (basic or fragments)"),
                    });
                }
            };
            if !(0..=100).contains(&ix.cardinality_threshold) {
                return Err(crate::de::DeError {
                    path: vec![
                        "indices".to_owned(),
                        i.to_string(),
                        "cardinalityThreshold".to_owned(),
                    ],
                    message: "must be between 0 and 100".to_owned(),
                });
            }
            indices.push(RelatedIndex {
                name: ix.name.to_lowercase(),
                kind,
                weight: ix.weight,
                cardinality_threshold: ix.cardinality_threshold,
                pattern: ix.pattern,
                to_lower: ix.to_lower || r.to_lower,
                apply_filter: ix.apply_filter,
            });
        }
        Ok(Self {
            threshold,
            include_newer: r.include_newer,
            to_lower: r.to_lower,
            indices,
        })
    }
}
