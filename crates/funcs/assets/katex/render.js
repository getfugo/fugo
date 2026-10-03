// The entry point of our `to_math` (crates/funcs/src/pure/katex.rs), evaluated after
// katex.min.js and mhchem.min.js. Rewritten from the Go implementation's
// internal/warpc/js/renderkatex.js and the console and error handling of
// internal/warpc/js/common.js (this repository at commit 44529028; Apache-2.0): one JSON
// message in ({expression, options}, as Go encodes warpc.KatexInput), one JSON message out
// ({output, warnings} or {err}).

// KaTeX reports through the console: \message, \show and missing character metrics go to
// console.log or console.warn, which Go's host discards; \errmessage goes to console.error,
// which common.js turns into an error.
globalThis.console = {
	log() {},
	warn() {},
	error(value) {
		throw new Error(value);
	},
};

// Go decodes the response with encoding/json, which replaces each lone surrogate with U+FFFD
// (KaTeX's error excerpts split surrogate pairs); toWellFormed does the same.
const wellFormed = (s) => (typeof s === 'string' ? s.toWellFormed() : s);

globalThis.renderKatex = function (json) {
	const data = JSON.parse(json);
	const options = data.options;
	const warnings = [];
	if (options.strict == 'warn') {
		// By default, KaTeX would write to console.warn.
		options.strict = (errorCode, errorMsg) => {
			warnings.push(
				`katex: LaTeX-incompatible input and strict mode is set to 'warn': ${errorMsg} [${errorCode}]`,
			);
		};
	}
	let result;
	try {
		const output = katex.renderToString(data.expression, options);
		result = { output: wellFormed(output), warnings: warnings.map(wellFormed) };
	} catch (e) {
		// common.js: the error message is the response's `err`.
		result = { err: wellFormed(e.message) };
	}
	return JSON.stringify(result);
};
