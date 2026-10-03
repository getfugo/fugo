import def from "./cjs-default.cjs";
import { named, other } from "./cjs-named.cjs";
import * as star from "./cjs-named.cjs";
import esm from "./cjs-esmodule.cjs";
import { live } from "./cjs-getter.cjs";

var required = require("./cjs-required.cjs");
var log = function () {
  console.log(Array.prototype.slice.call(arguments).join(" | "));
};

log("default", def(2, 3), def.version);
log("named", named, other, Object.keys(star).sort().join(","), typeof star.default);
log("esModule", esm, typeof esm);
log("getter", live);
log("required", required.value, required.fn());
import("./lazy.js").then(function (m) {
  log("lazy", m.lazy, m.default, Object.keys(m).sort().join(","));
});
import("./lazy-cjs.cjs").then(function (m) {
  log("lazy cjs", m.default.x, m.x);
});
