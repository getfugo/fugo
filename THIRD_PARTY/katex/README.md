# KaTeX 0.16.22 and its mhchem extension

`crates/funcs/assets/katex/katex.min.js` and `mhchem.min.js` are the KaTeX npm package's
`dist/katex.min.js` and `dist/contrib/mhchem.min.js` (katex 0.16.22), which `to_math` runs in
QuickJS as Hugo's `transform.ToMath` does. KaTeX is MIT-licensed (`LICENSE`, verbatim from the
package).

The mhchem extension (`contrib/mhchem/mhchem.js` in the package) says of itself: "This file
implements a KaTeX version of mhchem version 3.3.0. It is adapted from
MathJax/extensions/TeX/mhchem.js [...] This code, as other KaTeX code, is released under the
MIT license." The MathJax original carries "Copyright (c) 2011-2015 The MathJax Consortium,
Copyright (c) 2015-2018 Martin Hensel", licensed under the Apache License, Version 2.0 (the
text is the repository's `LICENSE`).
