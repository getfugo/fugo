//! `transform.Unmarshal`: a resource or a string decoded by format (JSON, TOML, YAML, CSV, XML, Org
//! front matter).

use super::*;

/// A data format of `unmarshal`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Json,
    Toml,
    Yaml,
    Csv,
    Xml,
}

impl Format {
    pub(super) fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "json" => Self::Json,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "csv" => Self::Csv,
            "xml" => Self::Xml,
            _ => return None,
        })
    }

    /// The format of a document: XML when it starts with `<`, else the first of `{`/`[`
    /// (JSON), `:` (YAML) and `=` (TOML), else CSV when it has a comma.
    pub(super) fn detect(s: &str) -> Option<Self> {
        let t = s.trim_start();
        if t.starts_with('<') {
            return Some(Self::Xml);
        }
        let first = |c: char| s.find(c).unwrap_or(usize::MAX);
        let json = if t.starts_with(['{', '[']) {
            0
        } else {
            usize::MAX
        };
        let (yaml, toml) = (first(':'), first('='));
        let min = json.min(yaml).min(toml);
        if min == usize::MAX {
            return s.contains(',').then_some(Self::Csv);
        }
        Some(if min == json {
            Self::Json
        } else if min == toml {
            Self::Toml
        } else {
            Self::Yaml
        })
    }

    pub(super) fn decode(self, s: &str) -> Result<ssg_base::Value, String> {
        use ssg_base::Value as Data;
        match self {
            Self::Json => Data::from_json_str(s).map_err(|e| e.to_string()),
            Self::Toml => Data::from_toml_str(s).map_err(|e| e.to_string()),
            Self::Yaml => Data::from_yaml_str(s).map_err(|e| e.to_string()),
            Self::Csv => csv_rows(s).map_err(|e| e.to_string()),
            Self::Xml => xml_root(s).map_err(|e| e.to_string()),
        }
    }
}

/// CSV (comma separated, rows of any length) as a list of lists of strings.
pub(super) fn csv_rows(text: &str) -> Result<ssg_base::Value, csv::Error> {
    use ssg_base::Value as Data;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        rows.push(Data::array(record?.iter().map(Data::string).collect()));
    }
    Ok(Data::array(rows))
}

/// An XML document as the value of its root element (attributes as `-name`, text beside
/// children or attributes as `#text`, repeated elements as lists), as `site.data` reads XML.
pub(super) fn xml_root(text: &str) -> Result<ssg_base::Value, roxmltree::Error> {
    let doc = roxmltree::Document::parse(text)?;
    Ok(xml_element(doc.root_element()))
}

pub(super) fn xml_element(e: roxmltree::Node<'_, '_>) -> ssg_base::Value {
    use ssg_base::Value as Data;
    let mut m = ssg_base::Map::new();
    for a in e.attributes() {
        m.insert(format!("-{}", a.name()), Data::string(a.value()));
    }
    let mut text = String::new();
    for child in e.children() {
        if child.is_element() {
            let name = child.tag_name().name();
            let v = xml_element(child);
            match m.get_mut(name) {
                None => {
                    m.insert(name, v);
                }
                Some(Data::Array(items)) => Arc::make_mut(items).push(v),
                Some(existing) => {
                    let first = std::mem::take(existing);
                    *existing = Data::array(vec![first, v]);
                }
            }
        } else if let Some(t) = child.text() {
            text.push_str(t);
        }
    }
    let text = text.trim();
    if m.is_empty() {
        return Data::string(text);
    }
    if !text.is_empty() {
        m.insert("#text", Data::string(text));
    }
    Data::map(m)
}

/// `x | unmarshal(format=?)`: a string or a resource's text parsed as JSON, TOML, YAML, CSV or
/// XML (`format`, else the resource's media type, else detected); map keys sorted.
pub(super) struct Unmarshal {
    pub(super) store: Arc<ResourceStore>,
}

impl SiteFilter for Unmarshal {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let explicit = match kw.get::<&str>("format")? {
            Some(f) => Some(Format::parse(f).ok_or_else(|| {
                msg(format!(
                    "unmarshal(format=\"{f}\"): expected json, toml, yaml, csv or xml"
                ))
            })?),
            None => None,
        };
        let (source, typed) = if field(&v, "__rid").is_some() {
            let id = resource_id(&self.store, &v, "unmarshal")?;
            let r = self.store.resource(id);
            let bytes = self.store.content(id).map_err(|e| chain("unmarshal", e))?;
            let s = String::from_utf8(bytes.to_vec())
                .map_err(|e| msg(format!("unmarshal: {} is not text: {e}", r.name)))?;
            let by_type = Format::parse(&r.media_type.sub).or_else(|| {
                r.name
                    .rsplit_once('.')
                    .and_then(|(_, ext)| Format::parse(ext))
            });
            (s, by_type)
        } else {
            (text(&v, "unmarshal")?, None)
        };
        if source.trim().is_empty() {
            return Ok(Value::from(tera::Map::new()));
        }
        let format = explicit
            .or(typed)
            .or_else(|| Format::detect(&source))
            .ok_or_else(|| msg("unmarshal: cannot detect the format of the input"))?;
        let data = format
            .decode(&source)
            .map_err(|e| msg(format!("unmarshal: {e}")))?;
        Ok(data.to_tera())
    }
}
