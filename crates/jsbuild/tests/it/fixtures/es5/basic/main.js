import { greet, sum, reexported } from "./lib.js";
import * as ns from "./ns.js";
import data from "./data.json";
import { nested } from "./data.json";
import text from "./hello.txt";
import blob from "./blob.bin";

var log = function () {
  console.log(Array.prototype.slice.call(arguments).join(" | "));
};

// Arrow functions capture this and arguments.
var obj = {
  v: 41,
  inc: function () { return [1, 2].map((x) => x + this.v); },
  args: function () { return (() => arguments.length)(); },
  nested: function () { var self = this; return (() => () => this === self)()(); },
  noParens: function () { return [3].map(x => x * 2)[0]; }
};
log("arrows", obj.inc().join(","), obj.args(1, 2, 3), obj.nested(), obj.noParens());

// Template literals, plain and tagged.
var who = "world", n = 3;
log(`hello ${who}`, `${n}${n}`, `a${n}b${who}c`, `plain`, `multi
line`, `nested ${`inner ${n}`}`, `${{ toString: function () { return "ts"; }, valueOf: function () { return "vo"; } }}`);
function tag(strings) {
  return strings.raw.join("/") + ":" + strings.join("/") + ":" + (arguments.length - 1);
}
function id(s) { return s; }
function site() { return id`a${0}b`; }
log("tagged", tag`x\n${1}y${2}`, site() === site(), Object.isFrozen(site()), tag`\unicode`);

// Shorthand properties and object spread.
var a = 1, b = 2;
var o1 = { a, b };
var o2 = { ...o1, c: 3, ...{ d: 4 } };
log("objects", JSON.stringify(o1), JSON.stringify(o2));

// ES2016+ operators.
var m = null, deep = { x: { y: 5 }, f: function () { return 7; } };
var la = null;
la ??= 1;
la ||= 2;
la &&= 3;
log("operators", 2 ** 10, m?.x, deep?.x?.y, deep.f?.(), m ?? "dflt", la, (m?.x)?.y);

// Optional catch binding.
try { throw new Error("x"); } catch { log("caught"); }

// Numeric literals, unicode escapes.
log("numbers", 0b1010, 0o17, 1_000_000, 0xFF, .5e1);
log("unicode", "\u{1F600}".length, "\u{61}", " ".length);

// Regular expressions newer than ES5.
log("regex", /a/y.test("a"), /\u{61}/u.test("a"), /a.b/s.test("a\nb"), /(?<y>\d+)/.exec("x12").groups.y, /a/gi.flags);

// Accessors.
var gs = { _v: 1, get v() { return this._v; }, set v(x) { this._v = x * 2; } };
gs.v = 5;
log("accessors", gs.v);

// Other module types.
log("json", data.name, nested.a, data.list.length, "text", text.trim(), "binary", blob.length, blob[0], blob[blob.length - 1]);
log("lib", greet("you"), sum(1, 2, 3), reexported, ns.answer, Object.keys(ns).sort().join(","));

// Block-level functions in strict code.
(function () {
  "use strict";
  {
    log("block fn", inner());
    function inner() { return "inner"; }
  }
})();

// Labels, switch, for-in.
outer: for (var i = 0; i < 3; i++) {
  for (var j = 0; j < 3; j++) {
    if (j === 1) continue outer;
    if (i === 2) break outer;
    log("loop", i, j);
  }
}
var keys = [];
for (var k in { p: 1, q: 2 }) keys.push(k);
switch (keys.length) { case 2: log("switch", keys.join("")); break; default: log("no"); }
