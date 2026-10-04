//! The kinds of resources, their bodies, origins and media types, digests, and call sites.

use super::*;

/// The options of `images.QR` (Go's defaults: medium, 4, no directory).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrOptions {
    pub level: QrLevel,
    /// Image pixels per module, at least 2.
    pub scale: u32,
    /// The directory of the image below the publish directory.
    pub target_dir: String,
}

impl Default for QrOptions {
    fn default() -> Self {
        Self {
            level: QrLevel::Medium,
            scale: 4,
            target_dir: String::new(),
        }
    }
}

/// Go's target path of `images.QR text options`: `<targetDir>/qr_<hash>.png`, the hash being
/// `hashing.HashStringHex(text, opts)` of the decoded options struct
/// `{Level string; Scale int; TargetDir string}` (hex without leading zeros).
#[must_use]
pub fn qr_target(text: &str, options: &QrOptions) -> String {
    let opts = gohash::structure(
        "",
        &[
            ("Level", gohash::string(options.level.name())),
            ("Scale", gohash::int(i64::from(options.scale))),
            ("TargetDir", gohash::string(&options.target_dir)),
        ],
    );
    let hash = gohash::list([gohash::string(text), opts]);
    paths::clean(&format!("/{}/qr_{hash:x}.png", options.target_dir))
}

/// What a resource is, for the template layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// A raster image the [`ImageQueue`] can process (JPEG, PNG, GIF, TIFF, BMP, WebP).
    Image,
    /// A page of a bundle (the site crate's; the store never creates one).
    Page(PageId),
    /// A text format (`text/*`, JSON, XML, SVG, JavaScript, TOML, YAML).
    Text,
    Other,
}

/// When a resource is written to the publish directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PublishPolicy {
    /// Always: bundle resources of rendered pages with `publishResources = true`.
    Eager,
    /// When its URL appears in a rendered output, or the `publish` filter marks it.
    OnReference,
    /// Never.
    Never,
}

/// Where a resource's bytes come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    File(PathBuf),
    Bytes(Arc<[u8]>),
    /// A processed image; the pixels come from the [`ImageQueue`].
    PendingImage(ImageOpId),
    /// A transform result not computed yet ([`Origin::Transformed`] names the source and the
    /// transform); [`ResourceStore::realize`] computes it.
    Pending,
}

/// How a resource was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `resources.Get`: the path inside the assets component (`css/a.css`).
    Asset { path: String },
    /// A bundle file of a page in `lang`.
    Bundle { lang: LangIdx },
    /// `resources.GetRemote`.
    Remote { url: String },
    /// `resources.FromString`, `resources.Concat`, `resources.Copy` or a template output.
    Named,
    /// A transform of another resource.
    Transformed {
        from: ResourceId,
        transform: Box<Transform>,
    },
    /// Another resource with front matter metadata (name, title, params) applied.
    Meta { from: ResourceId },
    /// A processed image of another resource.
    Image { from: ResourceId },
}

/// A hash algorithm of `fingerprint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HashAlgo {
    Md5,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgo {
    /// The name used in `integrity` values (`sha256`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
        }
    }

    pub(crate) fn digest(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => Md5::digest(bytes).to_vec(),
            Self::Sha256 => Sha256::digest(bytes).to_vec(),
            Self::Sha384 => Sha384::digest(bytes).to_vec(),
            Self::Sha512 => Sha512::digest(bytes).to_vec(),
        }
    }
}

impl std::str::FromStr for HashAlgo {
    type Err = ResourceError;

    /// `md5`, `sha256` (also the empty string), `sha384` or `sha512`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "" | "sha256" => Ok(Self::Sha256),
            "md5" => Ok(Self::Md5),
            "sha384" => Ok(Self::Sha384),
            "sha512" => Ok(Self::Sha512),
            other => Err(ResourceError::UnsupportedHash(other.to_owned())),
        }
    }
}

/// Where a call that names a target path was made: its language (sub-wave) and template
/// position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub lang: LangIdx,
    pub position: Option<Position>,
}

impl CallSite {
    /// A call in `lang` at an unknown position.
    #[must_use]
    pub const fn in_lang(lang: LangIdx) -> Self {
        Self {
            lang,
            position: None,
        }
    }
}

impl fmt::Display for CallSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.position {
            Some(p) => write!(f, "{p}"),
            None => f.write_str("<unknown position>"),
        }
    }
}

pub(crate) fn kind_of(mt: &MediaType) -> ResourceKind {
    if mt.main == "image" && ImageFormat::from_subtype(&mt.sub).is_some() {
        ResourceKind::Image
    } else if !mt.main.is_empty() && mt.is_text() {
        ResourceKind::Text
    } else {
        ResourceKind::Other
    }
}

pub(crate) fn empty_media_type() -> MediaType {
    MediaType {
        main: String::new(),
        sub: String::new(),
        mime_suffix: String::new(),
        suffixes: Vec::new(),
        delimiter: String::new(),
    }
}

/// Types of extensions the configured table does not know (the IANA registration first, then
/// `mime_guess`).
pub(super) fn well_known_media_type(ext: &str) -> Option<MediaType> {
    let t = match ext {
        "ico" => "image/vnd.microsoft.icon".to_owned(),
        _ => mime_guess::from_ext(ext).first_raw()?.to_owned(),
    };
    let mut mt = MediaType::parse(&t).ok()?;
    mt.suffixes = vec![ext.to_owned()];
    mt.delimiter = ".".to_owned();
    Some(mt)
}

/// A resource name for case-insensitive lookups: lower case, spaces as dashes.
#[must_use]
pub(crate) fn normalize_name(name: &str) -> String {
    ssg_base::text::to_lower(name).replace(' ', "-")
}

/// `ident` inserted before the extension of the last path element (`/a/b.css` + `.min` →
/// `/a/b.min.css`; a name without extension gets it at the end).
pub(crate) fn add_identifier(path: &str, ident: &str) -> String {
    let (dir, file) = paths::split(path);
    match file.rfind('.') {
        Some(i) => format!("{dir}{}{ident}{}", &file[..i], &file[i..]),
        None => format!("{dir}{file}{ident}"),
    }
}
