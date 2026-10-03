enum Color { Red, Green = "g", Blue = 4, Next }
enum Flags { A = 1 << 0, B = 1 << 1, AB = A | B }
enum Color { Extra = 9 }
namespace Geo {
  export var pi = 3.14;
  export function area(r: number): number { return pi * r * r; }
}
namespace Outer.Inner {
  export var deep: string = "deep";
}
interface Shape { kind: string }
type Fn = (...args: any[]) => void;
function describe(s: Shape, n?: number): string {
  return s.kind + (n !== undefined ? n : "") + (s as any).extra!;
}
var log = function () {
  console.log(Array.prototype.slice.call(arguments).join(" | "));
};
log("enum", Color.Red, Color[0], Color.Green, Color.Blue, Color.Next, Color[5], Color.Extra);
log("flags", Flags.AB, Flags[3]);
log("namespace", Geo.area(1), Outer.Inner.deep);
log("types", describe({ kind: "sq" } as Shape, 2));
