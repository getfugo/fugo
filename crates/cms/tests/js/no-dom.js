// The editor's bundle (assets/admin/cms.js) holds lit-html, which takes the page's `document`
// when it loads, and the Material components, which define their elements then. Tests of its other
// parts import this first: outside a browser, a document with nothing in it, an element class and
// a registry that keeps nothing are enough to load it (lit's own builds for node do the same). The
// tests of the editor's views draw it in happy-dom instead (editor-ui.test.js).

globalThis.document ??= { createTreeWalker: () => ({}), getElementById: () => null };
globalThis.HTMLElement ??= class HTMLElement {};
globalThis.customElements ??= { define() {}, get() {} };
globalThis.MutationObserver ??= class MutationObserver {};
globalThis.window ??= { matchMedia: () => ({ matches: false }) };
