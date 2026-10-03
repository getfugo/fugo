# Emoji data (via the `emojis` crate)

`emojify` (`crates/funcs`, T31) and the Markdown `emoji` extension (`crates/markup`, comrak's
`shortcodes` feature, T22) look emoji up with the [`emojis`](https://crates.io/crates/emojis)
crate, version 0.8.2. The crate is an ordinary dependency (licence
`(MIT OR Apache-2.0) AND Unicode-3.0`, checked by `tools/dev/licence-check.sh`); the data it
compiles into the `fugo` binary comes from two sources, whose licences are kept here:

| Data | Source | Licence |
|---|---|---|
| Emoji sequences, names, groups, Unicode versions (Unicode 17.0 emoji specification) | Unicode, Inc. (`emoji-test.txt`) | Unicode-3.0 ([`LICENSE-UNICODE`](LICENSE-UNICODE), copied verbatim from `emojis-0.8.2/LICENSE-UNICODE`) |
| GitHub short codes (`:smile:`) | [github/gemoji](https://github.com/github/gemoji) v4.1.0 | MIT ([`LICENSE-GEMOJI`](LICENSE-GEMOJI)) |

`LICENSE-GEMOJI` is the MIT licence text with gemoji's copyright line; it was written offline
(T70), so compare it with gemoji's `LICENSE` the next time the network is available.

Added by T70 (the entry in `THIRD_PARTY/README.md` dates from T22, the directory was missing).
