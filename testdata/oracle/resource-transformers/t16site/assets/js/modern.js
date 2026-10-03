const o = { a: 1, ...{ b: 2 } };
const v = o?.a ?? 3;
async function* gen() { yield await Promise.resolve(v); }
(async () => { for await (const x of gen()) console.log(x); })();
console.log(2 ** 10, [1, [2, [3]]].flat(Infinity), 1_000_000, o?.b?.c);
