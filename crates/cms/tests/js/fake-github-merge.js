// The fake GitHub's merges (`POST merges`): a three-way merge of trees, file by file, and of
// texts line by line when the three versions have as many lines. Unlike git, it does not follow
// moved files.

const CONFLICT = Symbol("conflict");

/** The tree (path → blob) of commits `ours` and `theirs` merged from their merge base `base`, or
 * null when they conflict. `gh`: the fake, for its trees and blobs. */
export function mergeTrees(gh, base, ours, theirs) {
  const [b, o, t] = [base, ours, theirs].map((c) => gh.trees.get(gh.commits.get(c).tree));
  const out = new Map();
  for (const p of new Set([...b.keys(), ...o.keys(), ...t.keys()])) {
    const blob = mergeBlob(gh, b.get(p), o.get(p), t.get(p));
    if (blob === CONFLICT) return null;
    if (blob !== undefined) out.set(p, blob);
  }
  return out;
}

/** A file's blob merged from its blobs at the merge base, ours and theirs (undefined: no file). */
function mergeBlob(gh, base, ours, theirs) {
  if (ours === base) return theirs;
  if (theirs === base || theirs === ours) return ours;
  if (base === undefined || ours === undefined || theirs === undefined) return CONFLICT;
  const text = (s) => gh.blobs.get(s).toString("utf8");
  const merged = mergeLines(text(base), text(ours), text(theirs));
  return merged === null ? CONFLICT : gh.putBlob(Buffer.from(merged));
}

/** Texts merged line by line: each line changed on one side only, or alike on both; else null. */
function mergeLines(base, ours, theirs) {
  const [b, o, t] = [base, ours, theirs].map((s) => s.split("\n"));
  if (o.length !== b.length || t.length !== b.length) return null;
  const out = [];
  for (let i = 0; i < b.length; i++) {
    if (o[i] === b[i]) out.push(t[i]);
    else if (t[i] === b[i] || t[i] === o[i]) out.push(o[i]);
    else return null;
  }
  return out.join("\n");
}
