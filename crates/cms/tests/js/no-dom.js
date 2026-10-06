// The editor's bundle (assets/admin/cms.js) holds lit-html, which takes the page's `document`
// when it loads. Tests of its other parts import this first: outside a browser, a document with
// nothing in it is enough to load it (lit-html's own build for node does the same). The tests of
// the editor's views draw it in happy-dom instead (editor-ui.test.js).

globalThis.document ??= { createTreeWalker: () => ({}), getElementById: () => null };
