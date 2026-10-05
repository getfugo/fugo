// The charts of the Benchmarks page (layouts/_shortcodes/benchmarks.html), from
// benchmarks/data.json: the history github-action-benchmark keeps, `entries` holding per suite
// one entry per commit, with that commit's results. Each `[data-benchmarks]` element draws one
// suite (bench-chart.js draws a chart):
//
// - the site builds (suite `fugo`): a chart per measurement; a result named `… (Go)` is the Go
//   implementation's build of the same site (measured once), a second line on the chart of `…`.
// - the Go implementation's micro-benchmarks (suite `Benchmark`, 2020 to 2025: time per call;
//   its `… - B/op` style copies are left out), with `data-with` naming the suite that continues
//   them: fugo's Rust version of a benchmark (and the Go benchmark, run again once next to it).
//   Charts with a Rust version come first; all are drawn as they scroll into view, and a field
//   filters them by name.

import { chart, day, html } from "./bench-chart.js";

const GO = " (Go)";
const KINDS = { fugo: 0, go: 1, history: 2 };

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

const plural = (n, word) => `${n.toLocaleString("en")} ${word}${n === 1 ? "" : "s"}`;

// The series of `entries` by benchmark name: { unit, points: [{ at, commit, date, value }] },
// `at` being the entry's index plus `offset`. A unit is its first word ("ns/op" of
// "ns/op  0 B/op  0 allocs/op").
function series(entries, offset = 0) {
  const byName = new Map();
  entries.forEach((entry, i) => {
    const commit = typeof entry.commit === "string" ? entry.commit : entry.commit?.id ?? "";
    for (const b of entry.benches) {
      if (!byName.has(b.name)) byName.set(b.name, { unit: String(b.unit).split(/\s/)[0], points: [] });
      byName.get(b.name).points.push({ at: offset + i, commit, date: entry.date, value: b.value });
    }
  });
  return byName;
}

// Adds the series of `byName` to `charts` as lines: `… (Go)` on the chart of `…`.
function addLines(charts, byName) {
  for (const [name, s] of byName) {
    const go = name.endsWith(GO);
    const key = go ? name.slice(0, -GO.length) : name;
    if (!charts.has(key)) charts.set(key, { name: key, lines: [] });
    charts.get(key).lines.push({ ...s, label: go ? "Go" : "fugo", kind: go ? "go" : "fugo" });
  }
}

function sorted(charts) {
  for (const c of charts.values()) c.lines.sort((a, b) => KINDS[a.kind] - KINDS[b.kind]);
  return [...charts.values()];
}

// The micro-benchmark charts: the Go history, continued by the suite `micro` from the end of it.
function microCharts(history, micro) {
  const charts = new Map();
  for (const [name, s] of series(history)) {
    if (!name.includes(" - ")) charts.set(name, { name, lines: [{ ...s, label: "Go", kind: "history" }] });
  }
  addLines(charts, series(micro, history.length));
  const rust = (c) => (c.lines.some((l) => l.kind === "fugo") ? 0 : 1);
  return sorted(charts).sort((a, b) => rust(a) - rust(b) || a.name.localeCompare(b.name));
}

// Charts drawn when they come near the viewport; the filter shows those whose name holds its text.
function lazyCharts(root, charts, total, commits, split) {
  const filter = html("input", "bench__filter");
  Object.assign(filter, { type: "search", placeholder: `Filter ${plural(charts.length, "benchmark")}` });
  filter.setAttribute("aria-label", "Filter the benchmarks by name");
  const grid = html("div", "bench__grid-charts");
  const pending = new WeakMap(); // placeholder → its chart
  const observer = new IntersectionObserver(
    (seen) => {
      for (const e of seen) {
        if (!e.isIntersecting || !pending.has(e.target)) continue;
        observer.unobserve(e.target);
        const figure = chart(pending.get(e.target), total, commits, split);
        figure.dataset.name = e.target.dataset.name;
        figure.hidden = e.target.hidden;
        e.target.replaceWith(figure);
      }
    },
    { rootMargin: "600px 0px" },
  );
  for (const c of charts) {
    const placeholder = html("div", "bench__placeholder", c.name);
    placeholder.dataset.name = c.name.toLowerCase();
    pending.set(placeholder, c);
    grid.append(placeholder);
    observer.observe(placeholder);
  }
  filter.addEventListener("input", () => {
    const text = filter.value.trim().toLowerCase();
    for (const node of grid.children) node.hidden = !node.dataset.name.includes(text);
  });
  root.append(filter, grid);
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
  const commits = root.dataset.commits;
  const span = (list) => `from ${day(list[0].date)} to ${day(list[list.length - 1].date)}`;
  if (root.dataset.with !== undefined) {
    const micro = data.entries[root.dataset.with] ?? [];
    const charts = microCharts(entries, micro);
    const rust = charts.filter((c) => c.lines.some((l) => l.kind === "fugo")).length;
    let text = `${plural(entries.length, "result")} of the Go implementation, ${span(entries)}`;
    if (micro.length) text += `; ${plural(micro.length, "result")} since, ${plural(rust, "benchmark")} with a Rust version`;
    root.append(html("p", "bench__range", `${text}.`));
    lazyCharts(root, charts, entries.length + micro.length, commits, micro.length ? entries.length : null);
    return;
  }
  const charts = new Map();
  addLines(charts, series(entries));
  const first = day(entries[0].date);
  const when = first === day(entries[entries.length - 1].date) ? `on ${first}` : span(entries);
  root.append(html("p", "bench__range", `${plural(entries.length, "commit")}, ${when}.`));
  const grid = html("div", "bench__grid-charts");
  for (const c of sorted(charts)) grid.append(chart(c, entries.length, commits));
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
