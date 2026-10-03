---
title: Imaging
description: Default quality, resampling filter, anchor, background colour and EXIF handling for image processing.
weight: 40
---

{{< code-toggle file=config >}}
[imaging]
  quality = 75
  resampleFilter = "box"
  anchor = "smart"
  bgColor = "#ffffff"
  hint = "photo"
  [imaging.exif]
    includeFields = ""
    excludeFields = ""
    disableDate = false
    disableLatLong = false
{{< /code-toggle >}}

`quality`
: JPEG and WebP quality, 1–100 (75).

`resampleFilter`
: The filter for resizing: `box` (default), `lanczos`, `catmullRom`, `mitchellNetravali`,
  `linear`, `nearestNeighbor` and others; see
  [Image processing](/content-management/image-processing/).

`anchor`
: Where `fill` and `crop` keep the image: `smart` (default), `center`, `topLeft`, …

`bgColor`
: The background for images converted to a format without transparency (JPEG).

`hint`
: The WebP encoding hint: `photo` (default), `picture`, `drawing`, `icon`, `text`.

`exif.includeFields`, `exif.excludeFields`
: Regular expressions of the EXIF tags to keep in `img | exif`. By default fugo leaves out
  large and technical tags.

`exif.disableDate`, `exif.disableLatLong`
: Do not read the date or the location.

Processed images are cached in `resources/_gen/images`; keep that directory between builds (or
commit it) to avoid processing images again. See [Caching](/configuration/caching/).
