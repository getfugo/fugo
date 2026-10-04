//! The languages: each one's configuration over the site's.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn languages(&self, root: &Map) -> Result<Languages, ConfigError> {
        let mut configured_tables = root
            .get("languages")
            .and_then(Value::as_map)
            .cloned()
            .unwrap_or_default();
        configured_tables.remove(merge::MERGE_KEY);
        let configured = !configured_tables.is_empty();
        let explicit_default = root
            .get("defaultcontentlanguage")
            .and_then(de::weak_string)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase());
        let mut tables: Vec<(String, Map)> = Vec::new();
        if configured {
            for (k, v) in configured_tables.iter() {
                match v {
                    Value::Map(m) => tables.push((k.to_lowercase(), (**m).clone())),
                    Value::Null => tables.push((k.to_lowercase(), Map::new())),
                    _ => {
                        return Err(self.locate(
                            ConfigError::invalid(format!("languages.{k}"), "expected a table"),
                            k,
                        ));
                    }
                }
            }
        } else {
            tables.push(("en".to_owned(), Map::new()));
        }
        let disable_list: Vec<String> = root
            .get("disablelanguages")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(de::weak_string)
                    .map(|s| s.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let multihost = tables.iter().any(|(_, t)| t.contains_key("baseurl"));
        let mut all: Vec<(i64, String, Map, bool)> = Vec::with_capacity(tables.len());
        for (key, table) in tables {
            let is_disabled = disable_list.contains(&key)
                || table
                    .get("disabled")
                    .and_then(de::weak_bool)
                    .unwrap_or(false);
            let weight = match table.get("weight") {
                None | Some(Value::Null) => 0,
                Some(v) => v
                    .as_i64()
                    .or_else(|| de::weak_string(v).and_then(|s| s.trim().parse().ok()))
                    .ok_or_else(|| {
                        self.locate(
                            ConfigError::invalid(
                                format!("languages.{key}.weight"),
                                "expected an integer",
                            ),
                            &key,
                        )
                    })?,
            };
            all.push((weight, key, table, is_disabled));
        }
        all.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        // Without `defaultContentLanguage`: `en` if configured, else the first language.
        let default = explicit_default.unwrap_or_else(|| {
            if all.iter().any(|(_, k, _, _)| k == "en") {
                "en".to_owned()
            } else {
                all.iter()
                    .find(|(.., d)| !d)
                    .map_or_else(|| "en".to_owned(), |(_, k, _, _)| k.clone())
            }
        });
        if !all.iter().any(|(_, k, _, _)| *k == default) {
            return Err(self.locate(
                ConfigError::invalid(
                    "defaultContentLanguage",
                    format_args!("{default:?} is not one of the configured languages"),
                ),
                &default,
            ));
        }
        let mut enabled: Vec<(i64, String, Map)> = Vec::new();
        let mut disabled = Vec::new();
        for (weight, key, table, is_disabled) in all {
            if is_disabled {
                if key == default {
                    return Err(ConfigError::invalid(
                        "disableLanguages",
                        format_args!("the default content language {key:?} cannot be disabled"),
                    ));
                }
                disabled.push(key);
            } else {
                enabled.push((weight, key, table));
            }
        }
        let pos = enabled
            .iter()
            .position(|(_, k, _)| *k == default)
            .expect("the default language is enabled");
        let first = enabled.remove(pos);
        enabled.insert(0, first);

        let mut base = root.clone();
        base.remove("languages");
        let mut keys = Vec::with_capacity(enabled.len());
        let mut trees = Vec::with_capacity(enabled.len());
        let mut own = std::collections::BTreeMap::new();
        for (_, key, table) in enabled {
            let mut t = base.clone();
            merge_language(&mut t, &table);
            keys.push(key.clone());
            trees.push(strip_map(&t));
            own.insert(key, table);
        }
        Ok(Languages {
            keys,
            trees,
            own,
            disabled,
            configured,
            multihost,
            in_subdir: root
                .get("defaultcontentlanguageinsubdir")
                .and_then(de::weak_bool)
                .unwrap_or(false),
        })
    }
}
