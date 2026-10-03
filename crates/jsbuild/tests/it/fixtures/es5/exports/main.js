import { helper } from "./helper.js";
export var x = 1;
export function f() { return helper() + x; }
export default { a: f() };
export { helper as renamed };
export * from "./more.js";
console.log("exports", f());
