---
title: Imaging
description: Default quality, resampling filter, anchor, background colour and EXIF handling for image processing, and the metadata of published images.
weight: 40
---

{{< code-toggle file=config >}}
[imaging]
  quality = 75
  resampleFilter = "box"
  anchor = "smart"
  bgColor = "#ffffff"
  hint = "photo"
  stripMetadata = true
  copyright = ""
  author = ""
  titleFromFileName = false
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

`stripMetadata`
: Publish JPEG, PNG and WebP images without their metadata (`true` by default; fugo's own
  setting). See [Image metadata](#image-metadata).

`copyright`, `author`, `titleFromFileName`
: A copyright notice (`{year}` in it is the build's year), an author and the file name as the
  title, written into every JPEG, PNG and WebP image fugo publishes, unless a page resource
  gives its own copyright and author (fugo's own settings; they need `stripMetadata`). See
  [Copyright notice](#copyright-notice).

`exif.includeFields`, `exif.excludeFields`
: Regular expressions of the EXIF tags to keep in `img | exif`. By default fugo leaves out
  large and technical tags.

`exif.disableDate`, `exif.disableLatLong`
: Do not read the date or the location.

Processed images are cached in `resources/_gen/images`; keep that directory between builds (or
commit it) to avoid processing images again. See [Caching](/configuration/caching/).

## Image metadata

A photo carries metadata: the camera, when it was taken and, from a phone, where (its GPS
position). Processed images (`resize`, `fill`, filters, …) never do. The images fugo publishes
as they are do: page resources, assets whose URL a page prints, and the files of `static/`.

By default, fugo removes from those JPEG, PNG and WebP files EXIF (camera, dates, GPS
position, thumbnail), XMP, IPTC, comments, PNG text chunks and other vendor data,
and anything appended after the image (such as the video of a motion photo). What changes how
the image looks stays: the colour profile and other colour information, and the EXIF
orientation, which a photo taken sideways needs to show upright.

Only the published copies change: your files keep their metadata, and `img | exif` still reads
it. Other formats (GIF, TIFF, BMP, AVIF, HEIC, …) are published as they are. Copies published
by an earlier build, and your Git history if the images are committed, keep their metadata.

To publish your images byte for byte as they are, as the Go implementation does, set
`stripMetadata = false`.

## Copyright notice

fugo can write a copyright notice, an author and a title into the JPEG, PNG and WebP images it
publishes, the processed ones included:

{{< code-toggle file=config >}}
[imaging]
  copyright = "© 2019–{year} by Jane Doe, all rights reserved"
  author = "Jane Doe"
  titleFromFileName = true
{{< /code-toggle >}}

They go into the image's EXIF (`Copyright` and `Artist`, which photo applications show) and
XMP (`dc:rights`, `dc:creator` and `dc:title`, which Google Images shows as the copyright
notice and the creator), in place of the image's own metadata: they need `stripMetadata`.
With `titleFromFileName`, `harbour_view.jpg` gets the title `harbour_view`, and an image
processed from it the same. Each setting holds at most 1000 bytes, without line breaks.

`{year}` in `copyright` is the year of the build: of `--clock` when it is given, else of the
system clock, in the default language's time zone (`timeZone`), the year `now()` gives its
templates. Built in 2027, the setting above writes `© 2019–2027 by Jane Doe, all rights
reserved`. It is the only placeholder: any other brace in `copyright`, such as a misspelt
`{yaer}`, is an error, so that a typo does not go into your images. The notice is written when
an image is published, after the [image cache](/configuration/caching/), so a new year does not
process the images again. The author is written as it is.

A photo by someone else, such as one from Wikimedia Commons under a Creative Commons licence,
needs its own notice. A page resource's `copyright` and `author` params
([metadata](/content-management/page-resources/#metadata)) are written into it, and into every
image processed from it, in place of `[imaging]`'s:

{{< code-toggle file=content/posts/harbour/index fm=true >}}
title = "The harbour"
[[resources]]
  src = "harbour.jpg"
  [resources.params]
    author = "Ada Example"
    copyright = "© Ada Example, CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/)"
{{< /code-toggle >}}

They replace both of `[imaging]`'s, so that the photo never carries your copyright next to its
author: one the resource does not set is not written. The title is still the file name
(`titleFromFileName`). Their values follow the rules of `[imaging]`'s: `{year}` is the build's
year, and each holds at most 1000 bytes of text without line breaks (in YAML, wrap a long one
with `>-`: `>` keeps a final line break); a value that breaks them, or is not text, stops the
build with an error naming the page and the resource. Like `[imaging]`'s, the notice is
written when the image is published: changing it processes no image again. Other params, such
as a `license` your templates print, are yours. With `stripMetadata = false`, images are
published as they are and these params do nothing.

A photo several pages publish carries one notice: that of the first of them, in language
order (the default content language first, then by weight), whose metadata gives one. A page
without the metadata takes nothing away. The translations of a bundle share its files, or each
publishes a copy of them (on a site with
[several hosts](/content-management/multilingual/#several-hosts), with
`[markup.goldmark] duplicateResourceFiles = true`, or for a content file that is not
Markdown): every copy carries that notice. So do the resources that
[content adapters](/content-management/content-adapters/) add for each language from one image.
