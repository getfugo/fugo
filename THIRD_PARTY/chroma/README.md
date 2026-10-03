# Chroma: lexers, styles and engine

`crates/highlight` is a port of [Chroma](https://github.com/alecthomas/chroma) v2.19.0, the
highlighter Hugo uses (MIT, `COPYING`):

- `crates/highlight/src/chroma/lexers/*.rs` are Chroma's XML lexers (`lexers/embedded/`),
  converted to Rust data (the same configuration and rules);
  `crates/highlight/src/chroma/golexers/exported/*.rs` are the rules of Chroma's lexers written
  in Go (`lexers/*.go`), exported in Chroma's XML format and converted likewise;
  `golexers/lisp.rs` transcribes the word lists of `lexers/cl.go` and `lexers/emacs.go`.
- `crates/highlight/src/styles/*.rs` are Chroma's styles (`styles/`), converted to Rust data.
- The engine (`crates/highlight/src/chroma/*.rs`) and the HTML formatter and style rules
  (`token.rs`, `style.rs`, `html.rs`) are rewritten from Chroma's Go sources.

Most of Chroma's lexers and styles were converted from [Pygments](https://pygments.org/)
(BSD-2-Clause, `LICENSE-PYGMENTS`, from Pygments 2.20.0; Chroma's lexers keep Pygments' rules,
regular expressions and keyword lists).
