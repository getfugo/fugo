//! Processing specs: `"600x400 webp q75 Lanczos Center #fff r90"` or the same as typed kwargs.
//!
//! A spec is a set of options. Each whitespace-separated token of the string form is, in this
//! order of precedence: an [`Action`], an [`Anchor`], a [`Resample`] filter, a [`Hint`], a
//! `#` background colour, `q<quality>` (1–100), `r<degrees>` (counter-clockwise rotation),
//! `<width>x<height>` (either side may be empty), or a target [`ImageFormat`] extension.
//! Tokens are case-insensitive. [`ImageSpec::resolve`] fills the unset options from the
//! site's [`Imaging`] defaults and the source format.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::color::Color;
use crate::error::ImageError;
use crate::format::ImageFormat;
use crate::settings::Imaging;

named_enum! {
    /// What a spec does to the image size.
    pub enum Action ("image action") {
        /// Scale to the given width and/or height (both given: exact size, aspect ignored).
        Resize = "resize",
        /// Cut a region of the given size at the anchor, without scaling.
        Crop = "crop",
        /// Scale to cover the box, then crop to it at the anchor.
        Fill = "fill",
        /// Scale down (never up) to fit inside the box, keeping the aspect ratio.
        Fit = "fit",
    }
}

named_enum! {
    /// Where [`Action::Crop`] and [`Action::Fill`] keep the image.
    pub enum Anchor ("anchor") {
        Center = "center",
        TopLeft = "topleft",
        Top = "top",
        TopRight = "topright",
        Left = "left",
        Right = "right",
        BottomLeft = "bottomleft",
        Bottom = "bottom",
        BottomRight = "bottomright",
        /// Content-aware: the Go implementation's smart crop (muesli/smartcrop; the crate README).
        /// Placed as [`Anchor::Center`] by [`Anchor::offset`], which planning does not use for it.
        Smart = "smart",
    }
}

named_enum! {
    /// The WebP encoder preset.
    pub enum Hint ("hint") {
        Picture = "picture",
        Photo = "photo",
        Drawing = "drawing",
        Icon = "icon",
        Text = "text",
    }
}

named_enum! {
    /// The resampling kernel of resizes (the Go implementation's names).
    pub enum Resample ("resample filter") {
        NearestNeighbor = "nearestneighbor",
        Box = "box",
        Linear = "linear",
        Hermite = "hermite",
        MitchellNetravali = "mitchellnetravali",
        CatmullRom = "catmullrom",
        BSpline = "bspline",
        Gaussian = "gaussian",
        Lanczos = "lanczos",
        Hann = "hann",
        Hamming = "hamming",
        Blackman = "blackman",
        Bartlett = "bartlett",
        Welch = "welch",
        Cosine = "cosine",
    }
}

impl Anchor {
    /// The top-left corner of a `w`×`h` box placed in a `src_w`×`src_h` image at this anchor.
    /// The box may be larger than the image (negative offsets); callers intersect.
    #[must_use]
    pub fn offset(self, (src_w, src_h): (u32, u32), (w, h): (u32, u32)) -> (i64, i64) {
        let (sw, sh, w, h) = (
            i64::from(src_w),
            i64::from(src_h),
            i64::from(w),
            i64::from(h),
        );
        // Integer division truncating toward zero, like the centre of a box that overhangs.
        let mid_x = (sw - w) / 2;
        let mid_y = (sh - h) / 2;
        match self {
            Self::TopLeft => (0, 0),
            Self::Top => (mid_x, 0),
            Self::TopRight => (sw - w, 0),
            Self::Left => (0, mid_y),
            Self::Right => (sw - w, mid_y),
            Self::BottomLeft => (0, sh - h),
            Self::Bottom => (mid_x, sh - h),
            Self::BottomRight => (sw - w, sh - h),
            Self::Center | Self::Smart => (mid_x, mid_y),
        }
    }
}

/// A processing spec as written (unset options take the site defaults, see
/// [`ImageSpec::resolve`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageSpec {
    pub action: Option<Action>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<ImageFormat>,
    pub quality: Option<u8>,
    pub hint: Option<Hint>,
    pub filter: Option<Resample>,
    pub anchor: Option<Anchor>,
    /// Counter-clockwise rotation in degrees, applied before the action.
    pub rotate: Option<i32>,
    /// Flatten transparent results onto this colour (the default background is used for
    /// formats without transparency).
    pub background: Option<Color>,
}

/// A spec with every option decided.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedSpec {
    pub action: Option<Action>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: ImageFormat,
    pub quality: u8,
    pub hint: Hint,
    pub filter: Resample,
    pub anchor: Anchor,
    /// Degrees counter-clockwise, normalised to `0..360`.
    pub rotate: u32,
    /// The background transparent results are flattened onto: the explicit one (`#rrggbb`
    /// token or kwarg), else the default one when the source format may be transparent and
    /// the target format cannot be.
    pub background: Option<Color>,
}

impl ImageSpec {
    /// A resize to `width`×`height` (`None` keeps the aspect ratio on that side).
    #[must_use]
    pub fn resize(width: Option<u32>, height: Option<u32>) -> Self {
        Self {
            action: Some(Action::Resize),
            width,
            height,
            ..Self::default()
        }
    }

    /// Fills the unset options from `imaging` and the `source` format.
    #[must_use]
    pub fn resolve(&self, imaging: &Imaging, source: ImageFormat) -> ResolvedSpec {
        let format = self.format.unwrap_or(source);
        // Converting a format that may be transparent to one that cannot flattens onto the
        // default background.
        let implied_background = (source.supports_transparency()
            && !format.supports_transparency())
        .then_some(imaging.background);
        ResolvedSpec {
            action: self.action,
            width: self.width,
            height: self.height,
            format,
            quality: self.quality.unwrap_or(imaging.quality),
            hint: self.hint.unwrap_or(imaging.hint),
            filter: self.filter.unwrap_or(imaging.resample),
            anchor: self.anchor.unwrap_or(imaging.anchor),
            rotate: self
                .rotate
                .map_or(0, |r| u32::try_from(r.rem_euclid(360)).unwrap_or(0)),
            background: self.background.or(implied_background),
        }
    }

    /// Parses the options of a spec string without [`validate`](Self::validate): for a spec
    /// that other options complete (`fill(spec="webp", width=300, height=200)`).
    ///
    /// # Errors
    /// An option that is not valid.
    pub fn parse_options(s: &str) -> Result<Self, ImageError> {
        let mut spec = Self::default();
        for token in s.split_whitespace() {
            let t = token.to_ascii_lowercase();
            if let Ok(a) = t.parse() {
                spec.action = Some(a);
            } else if let Ok(a) = t.parse() {
                spec.anchor = Some(a);
            } else if let Ok(f) = t.parse() {
                spec.filter = Some(f);
            } else if let Ok(h) = t.parse() {
                spec.hint = Some(h);
            } else if t.starts_with('#') {
                spec.background = Some(t.parse()?);
            } else if let Some(q) = t.strip_prefix('q') {
                let q: u8 = q
                    .parse()
                    .ok()
                    .filter(|q| (1..=100).contains(q))
                    .ok_or_else(|| ImageError::spec(s, format!("quality {q:?} is not 1–100")))?;
                spec.quality = Some(q);
            } else if let Some(r) = t.strip_prefix('r') {
                let r = r
                    .parse()
                    .map_err(|_| ImageError::spec(s, format!("rotation {r:?} is not degrees")))?;
                spec.rotate = Some(r);
            } else if t.contains('x') {
                (spec.width, spec.height) = Self::parse_dimensions(s, &t)?;
            } else if let Some(f) = ImageFormat::from_extension(&t) {
                spec.format = Some(f);
            } else {
                return Err(ImageError::spec(s, format!("unknown option {token:?}")));
            }
        }
        Ok(spec)
    }

    /// Checks that the size options fit the action.
    ///
    /// # Errors
    /// A size the action cannot use (a fill without a height, …).
    pub fn validate(self) -> Result<Self, ImageError> {
        let (w, h) = (self.width.is_some(), self.height.is_some());
        let reason = match self.action {
            Some(Action::Resize) if !w && !h => "resize needs a width or a height",
            Some(Action::Crop | Action::Fill | Action::Fit) if !w || !h => {
                "crop, fill and fit need a width and a height"
            }
            None if w || h => "a width or height needs an action (resize, crop, fill or fit)",
            _ => return Ok(self),
        };
        Err(ImageError::spec(self.to_string(), reason))
    }

    fn parse_dimensions(spec: &str, token: &str) -> Result<(Option<u32>, Option<u32>), ImageError> {
        let (w, h) = token.split_once('x').unwrap_or((token, ""));
        if h.contains('x') {
            return Err(ImageError::spec(
                spec,
                format!("{token:?} is not <width>x<height>"),
            ));
        }
        let side = |s: &str| -> Result<Option<u32>, ImageError> {
            if s.is_empty() {
                return Ok(None);
            }
            let v: u32 = s.parse().map_err(|_| {
                ImageError::spec(spec, format!("{s:?} in {token:?} is not a size in pixels"))
            })?;
            Ok((v > 0).then_some(v))
        };
        Ok((side(w)?, side(h)?))
    }
}

impl FromStr for ImageSpec {
    type Err = ImageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_options(s)?.validate()
    }
}

impl fmt::Display for ImageSpec {
    /// The canonical string form (options in a fixed order).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = self.action {
            parts.push(a.name().to_owned());
        }
        if self.width.is_some() || self.height.is_some() {
            let side = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_default();
            parts.push(format!("{}x{}", side(self.width), side(self.height)));
        }
        if let Some(v) = self.format {
            parts.push(v.to_string());
        }
        if let Some(v) = self.quality {
            parts.push(format!("q{v}"));
        }
        if let Some(v) = self.hint {
            parts.push(v.name().to_owned());
        }
        if let Some(v) = self.filter {
            parts.push(v.name().to_owned());
        }
        if let Some(v) = self.anchor {
            parts.push(v.name().to_owned());
        }
        if let Some(v) = self.rotate {
            parts.push(format!("r{v}"));
        }
        if let Some(v) = self.background {
            parts.push(v.to_string());
        }
        f.write_str(&parts.join(" "))
    }
}

/// The typed kwargs form of a spec (`resize(width=600, format="webp")`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecArgs {
    #[serde(default)]
    action: Option<Action>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    format: Option<ImageFormat>,
    #[serde(default)]
    quality: Option<u8>,
    #[serde(default)]
    hint: Option<Hint>,
    #[serde(default)]
    filter: Option<Resample>,
    #[serde(default)]
    anchor: Option<Anchor>,
    #[serde(default)]
    rotate: Option<i32>,
    #[serde(default)]
    background: Option<Color>,
}

impl TryFrom<SpecArgs> for ImageSpec {
    type Error = ImageError;

    fn try_from(a: SpecArgs) -> Result<Self, ImageError> {
        let spec = Self {
            action: a.action,
            width: a.width.filter(|&v| v > 0),
            height: a.height.filter(|&v| v > 0),
            format: a.format,
            quality: a.quality,
            hint: a.hint,
            filter: a.filter,
            anchor: a.anchor,
            rotate: a.rotate,
            background: a.background,
        };
        if let Some(q) = spec.quality
            && !(1..=100).contains(&q)
        {
            return Err(ImageError::spec(
                spec.to_string(),
                format!("quality {q} is not 1–100"),
            ));
        }
        spec.validate()
    }
}

impl Serialize for ImageSpec {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ImageSpec {
    /// A spec string, or a map of typed kwargs (`action`, `width`, `height`, `format`,
    /// `quality`, `hint`, `filter`, `anchor`, `rotate`, `background`).
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct SpecVisitor;

        impl<'de> serde::de::Visitor<'de> for SpecVisitor {
            type Value = ImageSpec;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an image spec string or a map of spec options")
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<ImageSpec, E> {
                s.parse().map_err(E::custom)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<ImageSpec, A::Error> {
                let args =
                    SpecArgs::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                ImageSpec::try_from(args).map_err(serde::de::Error::custom)
            }
        }

        d.deserialize_any(SpecVisitor)
    }
}
