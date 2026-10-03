let a = 1;
{ const b = a + 1; console.log(b); }
class C { static x = 1; #p = 2; get p() { return this.#p; } }
console.log(new C().p, C.x);
