export function greet(name) { return `hi ${name}`; }
export var sum = function () {
  var t = 0;
  for (var i = 0; i < arguments.length; i++) t += arguments[i];
  return t;
};
export * from "./reexp.js";
