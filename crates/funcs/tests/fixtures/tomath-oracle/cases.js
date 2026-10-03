// The to_math cases of tests/fixtures/tomath.jsonl.gz (crates/funcs/README.md, "to_math
// fixture"): one JSON line {"expression", "options"} per case on stdout, `options` the second
// argument of Go's transform.ToMath (null: none).
//
//   NODE_PATH=tools/dev/node_modules node cases.js <ss_data.yaml> <docs/content> >cases.jsonl
//
// ss_data.yaml is KaTeX's screenshotter corpus at the release the Go implementation bundles,
// https://raw.githubusercontent.com/KaTeX/KaTeX/v0.16.22/test/screenshotter/ss_data.yaml.
'use strict';
const fs = require('fs');
const path = require('path');
const YAML = require('yaml');

const [ssPath, docsDir] = process.argv.slice(2);
const cases = [];
const seen = new Set();
const add = (expression, options) => {
	const line = JSON.stringify({ expression, options: options ?? null });
	if (!seen.has(line)) {
		seen.add(line);
		cases.push(line);
	}
};

// KaTeX's screenshotter corpus: as the docs render math, and with ToMath's defaults.
for (const v of Object.values(YAML.parse(fs.readFileSync(ssPath, 'utf8')))) {
	const tex = typeof v === 'string' ? v : v.tex;
	const options = { output: 'htmlAndMathml', displayMode: typeof v === 'object' && !!v.display };
	if (typeof v === 'object' && v.macros) options.macros = v.macros;
	add(tex, options);
	add(tex, options.macros ? { macros: options.macros } : null);
}

// The formulas of the documentation (its passthrough delimiters), as its render hook calls
// ToMath, with the defaults, and without throwOnError.
const walk = (dir) =>
	fs
		.readdirSync(dir, { withFileTypes: true })
		.sort((a, b) => (a.name < b.name ? -1 : 1))
		.flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));
const delimiters = /\$\$([\s\S]+?)\$\$|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)/g;
for (const file of walk(docsDir).filter((f) => f.endsWith('.md'))) {
	for (const m of fs.readFileSync(file, 'utf8').matchAll(delimiters)) {
		const tex = m[1] ?? m[2] ?? m[3];
		add(tex, { output: 'htmlAndMathml', displayMode: m[3] === undefined });
		add(tex, null);
		add(tex, { output: 'htmlAndMathml', throwOnError: false });
	}
}

// mhchem (\ce, \pu): examples of its manual.
const mhchem = [
	'\\ce{H2O}', '\\ce{Sb2O3}', '\\ce{H+}', '\\ce{CrO4^2-}', '\\ce{[AgCl2]-}', '\\ce{Y^99+}',
	'\\ce{Y^{99+}}', '\\ce{2 H2O}', '\\ce{2H2O}', '\\ce{0.5 H2O}', '\\ce{1/2 H2O}', '\\ce{(1/2) H2O}',
	'\\ce{$n$ H2O}', '\\ce{^{227}_{90}Th+}', '\\ce{^227_90Th+}', '\\ce{^{0}_{-1}n^{-}}', '\\ce{^0_-1n-}',
	'\\ce{H{}^3HO}', '\\ce{H^3HO}', '\\ce{A -> B}', '\\ce{A <- B}', '\\ce{A <-> B}', '\\ce{A <--> B}',
	'\\ce{A <=> B}', '\\ce{A <=>> B}', '\\ce{A <<=> B}', '\\ce{A ->[H2O] B}',
	'\\ce{A ->[{text above}][{text below}] B}', '\\ce{A ->[$x$][$x_i$] B}', '\\ce{(NH4)2S}',
	'\\ce{[\\{(X2)3\\}2]^3+}', '\\ce{CH4 + 2 $\\left( \\ce{O2 + 79/21 N2} \\right)$}', '\\ce{H2(aq)}',
	'\\ce{CO3^2-_{(aq)}}', '\\ce{NaOH(aq,$\\infty$)}', '\\ce{ZnS($c$)}', '\\ce{ZnS(\\ca$c$)}', '\\ce{NO_x}',
	'\\ce{Fe^n+}', '\\ce{x Na(NH4)HPO4 ->[\\Delta] (NaPO3)_x + x NH3 ^ + x H2O}',
	'\\ce{Fe(CN)_{\\frac{6}{2}}}', '\\ce{X_{\\alpha}Y_{\\beta}}', '\\ce{Fe^{II}Fe^{III}2O4}',
	'\\ce{OCO^{.-}}', '\\ce{NO^{(2.)-}}', "\\ce{Li^x_{Li,1-2x}Mg^._{Li,x}$V$'_{Li,x}Cl^x_{Cl}}",
	"\\ce{O''_{i,x}}", '\\ce{M^{(.)}}', '\\ce{KCr(SO4)2*12H2O}', '\\ce{KCr(SO4)2.12H2O}',
	'\\ce{KCr(SO4)2 * 12 H2O}', '\\ce{Fe^{II}Fe^{III}_2O4}', '\\ce{A + B}', '\\ce{A - B}', '\\ce{A = B}',
	'\\ce{A \\pm B}', '\\ce{SO4^2- + Ba^2+ -> BaSO4 v}', '\\ce{A v B (v) -> B ^ B (^)}',
	'\\ce{Zn^2+  <=>[+ 2OH-][+ 2H+]  $\\underset{\\text{amphoteres Hydroxid}}{\\ce{Zn(OH)2 v}}$  <=>[+ 2OH-][+ 2H+]  $\\underset{\\text{Hydroxozikat}}{\\ce{[Zn(OH)4]^2-}}$}',
	'\\ce{$K = \\frac{[\\ce{Hg^2+}][\\ce{Hg}]}{[\\ce{Hg2^2+}]}$}', '\\ce{$K = \\ce{\\frac{[Hg^2+][Hg]}{[Hg2^2+]}}$}',
	'\\ce{Hg^2+ ->[I-]  $\\underset{\\mathrm{red}}{\\ce{HgI2}}$  ->[I-]  $\\underset{\\mathrm{red}}{\\ce{[Hg^{II}I4]^2-}}$}',
	'\\ce{H2O \\bond{~} H2O \\bond{-} H2O \\bond{=} H2O \\bond{#} H2O \\bond{...} H2O}', '\\ce{A-B=C#D}',
	'\\ce{CH3-CH=CH2}', '\\ce{HC#CH}', '\\ce{\\mu-Cl}', '\\ce{[Cu(NH3)4]^2+}', '\\ce{1s^2 2s^2}',
	'\\ce{Ca^{2+} + 2 OH- -> Ca(OH)2 v}', '\\pu{123 kJ}', '\\pu{123 mm2}', '\\pu{123 J s}', '\\pu{123 J*s}',
	'\\pu{123 kJ/mol}', '\\pu{123 kJ//mol}', '\\pu{123 kJ mol-1}', '\\pu{123 kJ*mol-1}', '\\pu{1.2e3 kJ}',
	'\\pu{1,2e3 kJ}', '\\pu{1.2E3 kJ}', '\\pu{1,2E3 kJ}', '\\pu{1.2x10^3 kJ}', '\\pu{1.2 * 10^3 kJ}',
	'\\pu{1.2*10^3 kJ}', '\\pu{12345.678}', '\\pu{-12345.678}', '\\pu{1.2-3.4 kJ}', '\\pu{75.3 J // mol K}',
	'C_p[\\ce{H2O(l)}] = \\pu{75.3 J // mol K}', '\\ce{A ->[\\text{Δ}] B}', '\\ce{Na+ + Cl- -> NaCl}',
	'\\ce{2H2 + O2 ->[\\text{spark}] 2H2O}', '\\ce{N2 + 3H2 <=> 2NH3}', '\\ce{A\\bond{-}B\\bond{=}C\\bond{#}D}',
];
for (const e of mhchem) {
	add(e, { output: 'htmlAndMathml' });
	add(e, { output: 'htmlAndMathml', displayMode: true });
	add(e, null);
}

// Options.
for (const e of ['a^2+b^2=c^2', '\\frac{a}{b}', '\\tag{1} x = y', '\\begin{equation} x \\tag{2} \\end{equation}',
	'\\sqrt[3]{x}', '\\overline{AB}']) {
	add(e, { output: 'htmlAndMathml', displayMode: true, leqno: true });
	add(e, { output: 'htmlAndMathml', displayMode: true, fleqn: true });
	add(e, { output: 'html', displayMode: true, fleqn: true, leqno: true });
	add(e, { output: 'htmlAndMathml', minRuleThickness: 0.1 });
	add(e, { output: 'html', minRuleThickness: 0 });
	add(e, { output: 'html', minRuleThickness: 0.06, displayMode: true });
}
add('\\RR \\f{x}{y}', { output: 'htmlAndMathml', macros: { '\\RR': '\\mathbb{R}', '\\f': '#1f(#2)' } });
add('\\gdef\\foo{bar} \\foo \\foo', { output: 'html' });
add('\\def\\foo{bar} \\foo', { output: 'mathml' });

// Errors: throwOnError, errorColor, strict.
for (const e of ['\\frac{a}{', '\\nosuchcommand{x}', 'x^^2', '\\begin{matrix} a \\end{pmatrix}', '\\ce{A ->',
	'\\text{😀}', '\\text{ü}', 'ก', '\\text{ก}', 'é', '\\errmessage{boom}', '\\message{hi} x', '\\show\\frac',
	'\\left( x', 'a & b', '\\\\', 'x \\\\ y', 'x \\newline y']) {
	add(e, null);
	add(e, { output: 'htmlAndMathml', throwOnError: false });
	add(e, { output: 'htmlAndMathml', throwOnError: false, errorColor: '#00ff00' });
	add(e, { output: 'htmlAndMathml', throwOnError: false, strict: 'warn' });
	add(e, { output: 'htmlAndMathml', throwOnError: true, strict: 'warn', displayMode: true });
	add(e, { output: 'htmlAndMathml', strict: 'ignore', displayMode: true });
}
for (const e of ['\\text{é} é', '\\text{\\\'e}', 'ก ข', '\\verb|x|', '\\color{red}{x}', '𝔸', '\\mathrm{𝔸}']) {
	add(e, { output: 'html', strict: 'warn' });
	add(e, { output: 'mathml', strict: 'ignore' });
	add(e, { output: 'mathml', strict: 'error' });
}

// mapstructure's weak decoding, unknown keys, escaping.
const weak = [
	['x', { output: 'html', displayMode: 'true' }], ['x', { output: 'html', displayMode: 1 }],
	['x', { output: 'html', displayMode: '1' }], ['x', { output: 'html', displayMode: 'false' }],
	['x', { output: 'html', displayMode: '' }], ['x', { output: 'html', displayMode: 0.5 }],
	['x', { output: 'html', displayMode: 'yes' }], ['\\frac{1}{2}', { output: 'html', minRuleThickness: '0.2' }],
	['\\frac{1}{2}', { output: 'html', minRuleThickness: true }], ['\\frac{1}{2}', { output: 'html', minRuleThickness: '' }],
	['\\frac{1}{2}', { output: 'html', minRuleThickness: 'abc' }], ['x', { output: 'html', strict: 'nope' }],
	['x', { output: 1 }], ['x', { output: true }], ['x', { output: 'nonsense' }], ['x', { output: 'HTML' }],
	['\\x', { output: 'html', macros: { '\\x': 1 } }], ['\\x', { output: 'html', macros: { '\\x': true } }],
	['\\x', { output: 'html', macros: { '\\x': 1.5 } }], ['\\x', { output: 'html', macros: [] }],
	['\\x', { output: 'html', macros: 'abc' }], ['x', { Output: 'html', DISPLAYMODE: true }],
	['x', { output: 'html', unknownKey: true, trust: true }], ['x', { output: null }],
	['x', { output: 'html', errorColor: 5 }], ['x', { output: 'html', throwOnError: 'false' }],
	['', { output: 'html' }], ['   ', { output: 'htmlAndMathml' }], ['x', { output: 'html', leqno: 't', fleqn: 'F' }],
	['\\href{https://x}{y}', { output: 'html' }], ['\\includegraphics{x.png}', { output: 'html', throwOnError: false }],
	['\\htmlClass{foo}{x}', { output: 'html', throwOnError: false }], ['\\url{https://example.org}', { output: 'html' }],
	['\\verb|<&>"\'|', { output: 'htmlAndMathml' }], ['\\text{<script>&amp;"\'}', { output: 'htmlAndMathml' }],
	['\\operatorname{<b>}', { output: 'htmlAndMathml' }], ['x <y> & z', { output: 'htmlAndMathml', throwOnError: false }],
];
for (const [e, o] of weak) add(e, o);

process.stdout.write(cases.join('\n') + '\n');
