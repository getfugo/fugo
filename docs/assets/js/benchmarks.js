// The charts of the Benchmarks page (layouts/_shortcodes/benchmarks.html). Each
// `[data-benchmarks]` element draws one suite of benchmarks/data.json: the history
// github-action-benchmark keeps, `entries` holding per suite one entry per commit, with the
// benchmarks of that commit. The charts are SVG drawn here, one per benchmark; a suite marked
// `data-search` shows one benchmark at a time, picked with a search field.

const SVG = "http://www.w3.org/2000/svg";
const W = 640;
const H = 220;
const PAD = { left: 64, right: 16, top: 16, bottom: 36 };

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
// { name, unit, points: [{ commit, date, value }] }. A unit is its first word ("ns/op" of
// "ns/op  0 B/op  0 allocs/op").
function series(entries) {
  const byName = new Map();
  for (const entry of entries) {
    const commit = typeof entry.commit === "string" ? entry.commit : entry.commit?.id ?? "";
    for (const b of entry.benches) {
      if (!byName.has(b.name)) {
        byName.set(b.name, { name: b.name, unit: String(b.unit).split(/\s/)[0], points: [] });
      }
      byName.get(b.name).points.push({ commit, date: entry.date, value: b.value });
    }
  }
  return [...byName.values()];
}

const number = (v) => (v >= 100 ? Math.round(v).toLocaleString("en") : String(Number(v.toPrecision(3))));
const day = (ms) => new Date(ms).toISOString().slice(0, 10);

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

// The chart of one series: a line over the commits, a point per commit linking to it.
function chart(s, commits) {
  const figure = html("figure", "bench__chart");
  const last = s.points[s.points.length - 1];
  const caption = html("figcaption", "bench__caption");
  caption.append(html("span", "bench__name", s.name), html("span", "bench__value", `${number(last.value)} ${s.unit}`));
  figure.append(caption);

  const box = svg("svg", { viewBox: `0 0 ${W} ${H}`, class: "bench__svg", role: "img" });
  box.setAttribute("aria-label", `${s.name}: ${s.points.length} results, the latest ${number(last.value)} ${s.unit}`);
  const [top, ticks] = axis(Math.max(...s.points.map((p) => p.value)));
  const width = W - PAD.left - PAD.right;
  const height = H - PAD.top - PAD.bottom;
  const n = s.points.length;
  const x = (i) => PAD.left + (n === 1 ? width / 2 : (i * width) / (n - 1));
  const y = (v) => PAD.top + height - (v / top) * height;

  for (const t of ticks) {
    svg("line", { class: "bench__grid", x1: PAD.left, x2: W - PAD.right, y1: y(t), y2: y(t) }, box);
    svg("text", { class: "bench__label", x: PAD.left - 8, y: y(t) + 4, "text-anchor": "end" }, box).textContent = number(t);
  }
  const dates = [[0, "start"], [n - 1, "end"]];
  for (const [i, anchor] of n === 1 ? [[0, "middle"]] : dates) {
    svg("text", { class: "bench__label", x: x(i), y: H - 12, "text-anchor": anchor }, box).textContent = day(s.points[i].date);
  }
  const line = s.points.map((p, i) => `${x(i).toFixed(1)},${y(p.value).toFixed(1)}`).join(" ");
  svg("polyline", { class: "bench__line", points: line }, box);
  s.points.forEach((p, i) => {
    const link = svg("a", { href: commits + p.commit }, box);
    svg("circle", { class: "bench__point", cx: x(i).toFixed(1), cy: y(p.value).toFixed(1), r: 3.5 }, link);
    svg("title", {}, link).textContent = `${day(p.date)}, ${p.commit.slice(0, 9)}: ${number(p.value)} ${s.unit}`;
  });
  figure.append(box);
  return figure;
}

// A search field over the series' names, and the chart of the one picked.
function searchable(root, list, commits) {
  const names = list.map((s) => s.name).sort();
  const id = `bench-names-${root.dataset.benchmarks}`;
  const field = html("input", "bench__search");
  Object.assign(field, { type: "search", placeholder: `Search ${names.length.toLocaleString("en")} benchmarks` });
  field.setAttribute("list", id);
  field.setAttribute("aria-label", "Benchmark");
  const options = html("datalist");
  options.id = id;
  for (const name of names) options.append(Object.assign(document.createElement("option"), { value: name }));
  const out = html("div", "bench__one");
  const show = (name) => {
    const s = list.find((x) => x.name === name);
    if (s) out.replaceChildren(chart(s, commits));
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
  const range = html("p", "bench__range", `${entries.length} commits, ${day(entries[0].date)} to ${day(entries[entries.length - 1].date)}.`);
  root.append(range);
  if (root.dataset.search !== undefined) {
    searchable(root, list, commits);
    return;
  }
  const grid = html("div", "bench__grid-charts");
  for (const s of list) grid.append(chart(s, commits));
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
