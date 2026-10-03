---
title: Image processing
description: Resize, crop and fill images, convert their format, apply filters, overlay text, read EXIF data and make QR codes — all built into fugo.
weight: 40
---

Images from a [page bundle](/content-management/page-resources/), from `assets/`
(`get_asset(path="images/logo.png")`) or from the web (`get_remote`) can be processed in
templates. The results are cached between builds and published under the image's directory
with a name like `cover_hu_3f2a9c….jpg`.

## Resize, fit, fill and crop

```html
{% set img = page.resources | get_resource(name="cover.jpg") %}
{% set small = img | resize(width=600) %}
<img src="{{ small.rel_permalink }}" width="{{ small.width }}" height="{{ small.height }}" alt="">
```

`resize`
: Scales to `width` and/or `height`; give one to keep the aspect ratio.

`fit`
: Scales down to fit inside `width`×`height`, keeping the aspect ratio.

`fill`
: Scales and crops to exactly `width`×`height`, keeping the part at `anchor`.

`crop`
: Crops to `width`×`height` at `anchor`, without scaling.

`process`
: Runs a spec with any action: `img | process(spec="fill 300x300 webp q80")`.

Every method takes `width`, `height`, `format`, `quality`, `filter`, `anchor` as keyword
arguments, or one `spec` string, as in Go templates:

```html
{% set card = img | fill(spec="600x400 webp q75 Lanczos Center") %}
<img src="{{ card.rel_permalink }}" width="{{ card.width }}" height="{{ card.height }}" alt="">
```

## The spec

A spec is a list of options separated by spaces, in any order:

Size
: `600x400`, `600x` (width only), `x400` (height only).

Format
: `jpg`, `png`, `gif`, `tiff`, `bmp` or `webp`. Default: the source's format.

Quality
: `q75`, for JPEG and lossy WebP (1–100; default `[imaging] quality`, 75).

Hint
: For WebP: `photo` (default), `picture`, `drawing`, `icon`, `text`.

Resample filter
: `Box`, `Lanczos`, `CatmullRom`, `MitchellNetravali`, `Linear`, `NearestNeighbor`, `Hermite`,
  `BSpline`, `Gaussian`, `Hann`, `Hamming`, `Blackman`, `Bartlett`, `Welch`, `Cosine`.
  Default: `[imaging] resampleFilter`, `box`.

Anchor
: For `fill` and `crop`: `Smart` (the default: the most interesting region, found by detecting
  edges, skin tones and saturation), `Center`, `TopLeft`, `Top`, `TopRight`, `Left`, `Right`,
  `BottomLeft`, `Bottom`, `BottomRight`.

Rotation
: `r90`, `r180`, `r270` (counter-clockwise).

Background
: `#fff`, `#ffaa00`: the colour of transparent areas when the target format has none (JPEG).

A photo whose camera stored its orientation in EXIF data is processed as stored; apply the
`auto_orient` [filter](#filters) first to turn it upright.

## Filters

`image_filter` applies filters in order. Each is a map with an `op`:

```text
{% set fx = img | image_filter(filters=[
  {"op": "grayscale"},
  {"op": "gaussian_blur", "sigma": 2},
  {"op": "process", "spec": "resize 800x webp"}
]) %}
```

| Filter | Options |
|---|---|
| `brightness` | `percentage` (−100…100) |
| `contrast` | `percentage` (−100…100) |
| `gamma` | `gamma` (above 1 brightens) |
| `saturation` | `percentage` (−100…500) |
| `hue` | `shift` (degrees, −180…180) |
| `colorize` | `hue`, `saturation`, `percentage` |
| `color_balance` | `r`, `g`, `b` (percentages) |
| `sepia` | `percentage` (0…100) |
| `sigmoid` | `midpoint` (0…1), `factor` (−10…10) |
| `grayscale`, `invert`, `auto_orient` | — |
| `gaussian_blur` | `sigma` |
| `unsharp_mask` | `sigma`, `amount`, `threshold` |
| `pixelate` | `size` |
| `opacity` | `opacity` (0…1) |
| `padding` | `top`, `right`, `bottom`, `left`, `color` |
| `overlay` | `image` (another image), `x`, `y` |
| `mask` | `image`: its luminance becomes the alpha |
| `text` | `text`, `color`, `size`, `x`, `y`, `alignx`, `aligny`, `linespacing`, `font` |
| `dither` | `colors`, `method`, `serpentine`, `strength` |
| `process` | `spec`: a processing spec inside the chain |

### Text

The `text` filter draws text, wrapped to the image's width:

```text
{% set font = get_remote(url="https://github.com/google/fonts/raw/main/ofl/lato/Lato-Bold.ttf") %}
{% set card = get_asset(path="images/card.png") | image_filter(filters=[
  {"op": "text", "text": page.title, "font": font, "size": 64, "color": "#ffffff", "x": 80, "y": 120}
]) %}
```

Without `font`, the Go font is used. This documentation does not need it, but sites use it for
social cards with the page title.

## Image information

`img.width`, `img.height`
: The size in pixels.

`img | exif`
: The EXIF data of a JPEG or TIFF: `date`, `lat`, `long` and `tags`, filtered by
  `[imaging.exif]`; none when the image has none.

## QR codes

```html
{% set qr = qr_code(text=page.permalink, level="medium", scale=4) %}
<img src="{{ qr.rel_permalink }}" width="{{ qr.width }}" height="{{ qr.height }}" alt="QR code of this page">
```

The [`qr` shortcode](/content-management/shortcodes/#qr) does the same in content.

## Configuration

`[imaging]` sets the defaults: `quality`, `resampleFilter`, `anchor`, `bgColor`, `hint`, and
`[imaging.exif]` what EXIF data is read. See [Imaging](/configuration/imaging/).
