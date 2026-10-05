// The charts of the Benchmarks page (layouts/_shortcodes/benchmarks.html). Each
// `[data-benchmarks]` element draws one suite of benchmarks/data.json: the history
// github-action-benchmark keeps, `entries` holding per suite one entry per commit, with the
// benchmarks of that commit. The charts are SVG drawn here, one per measurement: a benchmark
// named `… (Go)` is the Go implementation's build of the same site in the same run, drawn as a
// second line on the chart of `…`. A suite marked `data-search` shows one benchmark at a time,
// picked with a search field.

const SVG = "http://www.w3.org/2000/svg";
const W = 640;
const H = 220;
const PAD = { left: 64, right: 16, top: 16, bottom: 36 };
const GO = " (Go)";

const loads = new Map(); // URL → Promise of the results (null when there are none)

function load(src) {
  if (!loads.has(src)) {
    const results = fetch(src)
      .then((r) => (r.ok ? r.json() : null))
      .catch(() => null);
    loads.set(src, results);
  }
  return loads.get(src);
}

// One series per benchmark, in the order the benchmarks first appear:
// { name, unit, points: [{ at, commit, date, value }] }, `at` being the entry's index. A unit is
// its first word ("ns/op" of "ns/op  0 B/op  0 allocs/op").
function series(entries) {
  const byName = new Map();
  entries.forEach((entry, at) => {
    const commit = typeof entry.commit === "string" ? entry.commit : entry.commit?.id ?? "";
    for (const b of entry.benches) {
      if (!byName.has(b.name)) {
        byName.set(b.name, { name: b.name, unit: String(b.unit).split(/\s/)[0], points: [] });
      }
      byName.get(b.name).points.push({ at, commit, date: entry.date, value: b.value });
    }
  });
  return [...byName.values()];
}

// The charts: each measurement with fugo's line, then the Go implementation's when there is one.
function charts(list) {
  const byName = new Map();
  for (const s of list) {
    const go = s.name.endsWith(GO);
    const name = go ? s.name.slice(0, -GO.length) : s.name;
    if (!byName.has(name)) byName.set(name, { name, lines: [] });
    byName.get(name).lines.push({ ...s, label: go ? "Go" : "fugo", go });
  }
  for (const c of byName.values()) c.lines.sort((a, b) => a.go - b.go);
  return [...byName.values()];
}

const number = (v) => (v >= 100 ? Math.round(v).toLocaleString("en") : String(Number(v.toPrecision(3))));
const day = (ms) => new Date(ms).toISOString().slice(0, 10);
const plural = (n, word) => `${n.toLocaleString("en")} ${word}${n === 1 ? "" : "s"}`;

function svg(name, attrs, parent) {
  const node = document.createElementNS(SVG, name);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  if (parent) parent.append(node);
  return node;
}

function html(name, className, text) {
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

// The caption: the measurement, and the latest value of each line (with its colour when there
// are two).
function caption(c) {
  const node = html("figcaption", "bench__caption");
  node.append(html("span", "bench__name", c.name));
  const values = html("span", "bench__values");
  for (const line of c.lines) {
    const last = line.points[line.points.length - 1];
    const kind = line.go ? " bench__value--go" : c.lines.length > 1 ? " bench__value--fugo" : "";
    const value = html("span", `bench__value${kind}`);
    value.textContent = `${c.lines.length > 1 ? `${line.label} ` : ""}${number(last.value)} ${line.unit}`;
    values.append(value);
  }
  node.append(values);
  return node;
}

// The chart of one measurement over the suite's `total` entries: a line per series, a point per
// commit linking to it.
function chart(c, total, commits) {
  const figure = html("figure", "bench__chart");
  figure.append(caption(c));
  const box = svg("svg", { viewBox: `0 0 ${W} ${H}`, class: "bench__svg", role: "img" });
  const latest = c.lines.map((l) => `${l.label} ${number(l.points[l.points.length - 1].value)} ${l.unit}`);
  box.setAttribute("aria-label", `${c.name}, the latest: ${latest.join(", ")}`);
  const all = c.lines.flatMap((l) => l.points);
  const [top, ticks] = axis(Math.max(...all.map((p) => p.value)));
  const width = W - PAD.left - PAD.right;
  const height = H - PAD.top - PAD.bottom;
  const first = Math.min(...all.map((p) => p.at));
  const span = total - 1 - first;
  const x = (at) => PAD.left + (span === 0 ? width / 2 : ((at - first) * width) / span);
  const y = (v) => PAD.top + height - (v / top) * height;

  for (const t of ticks) {
    svg("line", { class: "bench__grid", x1: PAD.left, x2: W - PAD.right, y1: y(t), y2: y(t) }, box);
    svg("text", { class: "bench__label", x: PAD.left - 8, y: y(t) + 4, "text-anchor": "end" }, box).textContent = number(t);
  }
  const date = (at) => all.find((p) => p.at === at)?.date;
  const ends = span === 0 ? [[first, "middle"]] : [[first, "start"], [total - 1, "end"]];
  for (const [at, anchor] of ends) {
    if (date(at) === undefined) continue;
    svg("text", { class: "bench__label", x: x(at), y: H - 12, "text-anchor": anchor }, box).textContent = day(date(at));
  }
  for (const line of c.lines) {
    const go = line.go ? " bench__line--go" : "";
    const points = line.points.map((p) => `${x(p.at).toFixed(1)},${y(p.value).toFixed(1)}`).join(" ");
    svg("polyline", { class: `bench__line${go}`, points }, box);
    for (const p of line.points) {
      const link = svg("a", { href: commits + p.commit }, box);
      const point = line.go ? "bench__point bench__point--go" : "bench__point";
      svg("circle", { class: point, cx: x(p.at).toFixed(1), cy: y(p.value).toFixed(1), r: 3.5 }, link);
      const label = c.lines.length > 1 ? `${line.label}, ` : "";
      svg("title", {}, link).textContent = `${label}${day(p.date)}, ${p.commit.slice(0, 9)}: ${number(p.value)} ${line.unit}`;
    }
  }
  figure.append(box);
  return figure;
}

// A search field over the benchmarks' names, and the chart of the one picked.
function searchable(root, list, total, commits) {
  const all = charts(list);
  const names = all.map((c) => c.name).sort();
  const id = `bench-names-${root.dataset.benchmarks}`;
  const field = html("input", "bench__search");
  Object.assign(field, { type: "search", placeholder: `Search ${plural(names.length, "benchmark")}` });
  field.setAttribute("list", id);
  field.setAttribute("aria-label", "Benchmark");
  const options = html("datalist");
  options.id = id;
  for (const name of names) options.append(Object.assign(document.createElement("option"), { value: name }));
  const out = html("div", "bench__one");
  const show = (name) => {
    const c = all.find((x) => x.name === name);
    if (c) out.replaceChildren(chart(c, total, commits));
  };
  field.addEventListener("input", () => show(field.value));
  root.append(field, options, out);
  show(names[0]);
}

function draw(root, data) {
  const status = root.querySelector("[data-bench-status]");
  const entries = data?.entries?.[root.dataset.benchmarks];
  if (!entries || entries.length === 0) {
    status.textContent = data
      ? "No results in this suite yet."
      : "This build of the site has no benchmark results; the published site has them.";
    return;
  }
  status.remove();
  const list = series(entries);
  const commits = root.dataset.commits;
  const first = day(entries[0].date);
  const last = day(entries[entries.length - 1].date);
  const range = first === last ? `on ${first}` : `from ${first} to ${last}`;
  root.append(html("p", "bench__range", `${plural(entries.length, "commit")}, ${range}.`));
  if (root.dataset.search !== undefined) {
    searchable(root, list, entries.length, commits);
    return;
  }
  const grid = html("div", "bench__grid-charts");
  for (const c of charts(list)) grid.append(chart(c, entries.length, commits));
  root.append(grid);
}

// Draws the charts of the page; runs again after each client-side navigation.
export function initBenchmarks() {
  for (const root of document.querySelectorAll("[data-benchmarks]")) {
    if (root.dataset.drawn) continue;
    root.dataset.drawn = "true";
    load(root.dataset.src).then((data) => draw(root, data));
  }
}
