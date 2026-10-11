# image-metadata: original images published without their metadata

`[imaging] stripMetadata`, on by default, publishes the JPEG, PNG and WebP files that are not
processed (page resources, assets, static files) without their metadata
(`ssg_images::strip_metadata`): EXIF with the GPS position, XMP, IPTC, comments, PNG text
chunks. The Go build published them byte for byte, so the docs sites' static PNG icons, whose
text chunks are gone, differ at L4; their pixels are the same. `stripMetadata = false` gives
the Go build's bytes.

- docs-i01 L4 `android-chrome-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-i01 L4 `apple-touch-icon.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-i01 L4 `favicon-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-i01 L4 `mstile-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-reduced L4 `android-chrome-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-reduced L4 `apple-touch-icon.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-reduced L4 `favicon-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-reduced L4 `mstile-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-live L4 `android-chrome-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-live L4 `apple-touch-icon.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-live L4 `favicon-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
- docs-live L4 `mstile-*.png` accepted-deviation: published without its text chunks (`[imaging] stripMetadata`, on by default)
