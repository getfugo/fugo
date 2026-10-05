// One chart of the Benchmarks page (benchmarks.js): an SVG line per series of a measurement,
// on an axis of results (`at`: a result's position in the suite, or in the Go history followed
// by the suite), with the latest values and fugo's speed-up against Go in the caption.
//
// A chart: { name, lines: [{ label, kind, unit, points: [{ at, commit, date, value }] }] }, kind
// "fugo", "go" (the Go implementation next to fugo: measured once, on 2026-10-05, its last result
// continues as a reference) or "history" (the Go micro-benchmarks, 2020 to 2025).

const SVG = "http://www.w3.org/2000/svg";
const W = 640;
const H = 220;
const PAD = { left: 64, right: 16, top: 22, bottom: 36 };

export const number = (v) => (v >= 100 ? Math.round(v).toLocaleString("en") : String(Number(v.toPrecision(3))));
export const day = (ms) => new Date(ms).toISOString().slice(0, 10);

function svg(name, attrs, parent) {
  const node = document.createElementNS(SVG, name);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  if (parent) parent.append(node);
  return node;
}

export function html(name, className, text) {
  const node = document.createElement(name);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// The y axis: from 0 to a round value at or above `max`, with a tick at each half.
function axis(max) {
  if (!(max > 0)) return [1, [0, 0.5, 1]];
  const step = 10 ** Math.floor(Math.log10(max));
  const top = [1, 2, 2.5, 5, 10].map((m) => m * step).find((t) => t >= max) ?? 10 * step;
  return [top, [0, top / 2, top]];
}

const last = (line) => line.points[line.points.length - 1];

// fugo against Go, by their latest results: "3.3× faster" (or slower, or less or more memory).
function ratio(c) {
  const fugo = c.lines.find((l) => l.kind === "fugo");
  const go = c.lines.find((l) => l.kind === "go") ?? c.lines.find((l) => l.kind === "history");
  if (!fugo || !go || !(last(fugo).value > 0) || !(last(go).value > 0)) return null;
  const r = last(go).value / last(fugo).value;
  if (Math.abs(r - 1) < 0.05) return ["about the same", null];
  const memory = fugo.unit === "MiB";
  const better = memory ? "less memory" : "faster";
  const worse = memory ? "more memory" : "slower";
  return r >= 1 ? [`${r.toFixed(1)}× ${better}`, true] : [`${(1 / r).toFixed(1)}× ${worse}`, false];
}

function caption(c, total) {
  const node = html("figcaption", "bench__caption");
  node.append(html("span", "bench__name", c.name));
  const values = html("span", "bench__values");
  const shown = c.lines.filter((l) => l.kind !== "history" || !c.lines.some((o) => o.kind === "go"));
  for (const line of shown) {
    const label = shown.length > 1 ? `${line.label} ` : "";
    // Go measured once: its result is a reference, dated.
    const when = line.kind === "go" && last(line).at < total - 1 ? ` (${day(last(line).date)})` : "";
    const text = `${label}${number(last(line).value)} ${line.unit}${when}`;
    values.append(html("span", `bench__value bench__value--${line.kind}`, text));
  }
  const r = ratio(c);
  if (r) {
    const tone = r[1] === null ? " bench__ratio--same" : r[1] ? "" : " bench__ratio--worse";
    values.append(html("span", `bench__ratio${tone}`, r[0]));
  }
  node.append(values);
  return node;
}

// Draws chart `c` into a new figure. `total`: the number of results on the axis; `split`: where
// the suite starts after the Go history (a marked line), or null.
export function chart(c, total, commits, split = null) {
  const figure = html("figure", "bench__chart");
  figure.append(caption(c, total));
  const box = svg("svg", { viewBox: `0 0 ${W} ${H}`, class: "bench__svg", role: "img" });
  const latest = c.lines.map((l) => `${l.label} ${number(last(l).value)} ${l.unit}`);
  box.setAttribute("aria-label", `${c.name}, the latest: ${latest.join(", ")}`);
  const all = c.lines.flatMap((l) => l.points);
  const [top, ticks] = axis(Math.max(...all.map((p) => p.value)));
  const width = W - PAD.left - PAD.right;
  const height = H - PAD.top - PAD.bottom;
  const first = Math.min(...all.map((p) => p.at));
  const span = total - 1 - first;
  const x = (at) => PAD.left + (span <= 0 ? width / 2 : ((at - first) * width) / span);
  const y = (v) => PAD.top + height - (v / top) * height;

  for (const t of ticks) {
    svg("line", { class: "bench__grid", x1: PAD.left, x2: W - PAD.right, y1: y(t), y2: y(t) }, box);
    svg("text", { class: "bench__label", x: PAD.left - 8, y: y(t) + 4, "text-anchor": "end" }, box).textContent = number(t);
  }
  const date = (at) => all.find((p) => p.at === at)?.date;
  const ends = span <= 0 ? [[first, "middle"]] : [[first, "start"], [total - 1, "end"]];
  for (const [at, anchor] of ends) {
    if (date(at) !== undefined) {
      svg("text", { class: "bench__label", x: x(at), y: H - 12, "text-anchor": anchor }, box).textContent = day(date(at));
    }
  }
  if (split !== null && split > first && all.some((p) => p.at >= split)) {
    const at = x(split - 0.5);
    svg("line", { class: "bench__split", x1: at, x2: at, y1: PAD.top - 14, y2: H - PAD.bottom }, box);
    // Left of the marker when it is near the right edge.
    const right = at > W - PAD.right - 60;
    const label = { class: "bench__label", x: right ? at - 4 : at + 4, y: PAD.top - 6, "text-anchor": right ? "end" : "start" };
    svg("text", label, box).textContent = "Rust →";
  }
  // Go's history first, fugo's line on top.
  for (const line of [...c.lines].reverse()) {
    const kind = `bench__line--${line.kind}`;
    const points = line.points.map((p) => `${x(p.at).toFixed(1)},${y(p.value).toFixed(1)}`).join(" ");
    svg("polyline", { class: `bench__line ${kind}`, points }, box);
    // The Go implementation was measured once: its last result runs on as a reference.
    const end = last(line);
    if (line.kind === "go" && end.at < total - 1) {
      const ref = { class: `bench__line ${kind}`, x1: x(end.at), y1: y(end.value), x2: x(total - 1), y2: y(end.value) };
      svg("line", ref, box);
    }
    const r = line.points.length <= 3 ? 5 : line.points.length > 40 ? 2 : 3.5;
    for (const p of line.points) {
      const link = svg("a", { href: commits + p.commit }, box);
      svg("circle", { class: `bench__point bench__point--${line.kind}`, cx: x(p.at).toFixed(1), cy: y(p.value).toFixed(1), r }, link);
      const label = c.lines.length > 1 ? `${line.label}, ` : "";
      svg("title", {}, link).textContent = `${label}${day(p.date)}, ${p.commit.slice(0, 9)}: ${number(p.value)} ${line.unit}`;
    }
  }
  figure.append(box);
  return figure;
}
