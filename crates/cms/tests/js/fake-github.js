// An in-memory GitHub for the Worker's tests: the REST calls and GraphQL queries the Worker
// makes, with git semantics (snapshots as trees, fast-forward ref updates, merge bases).

import { createHash, createVerify } from "node:crypto";

const sha1 = (s) => createHash("sha1").update(s).digest("hex");
const jsonResponse = (status, body) =>
  new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

export class FakeGitHub {
  /** `files`: path → text of the first commit of `branch`. */
  constructor({ repo = "owner/site", branch = "main", files = {}, token = "test-token" } = {}) {
    this.repo = repo;
    this.token = token;
    this.blobs = new Map();
    this.trees = new Map();
    this.commits = new Map();
    this.refs = new Map();
    this.clock = Date.parse("2026-01-01T00:00:00Z");
    this.calls = [];
    this.app = null;
    /** At most this many files in a compare (GitHub: 300). */
    this.compareLimit = Infinity;
    /** Called after each REST call with (method, path): a push by someone else, in between. */
    this.after = null;
    const tree = this.putTree(new Map(Object.entries(files).map(([p, t]) => [p, this.putBlob(Buffer.from(t))])));
    this.refs.set(branch, this.putCommit({ tree, parents: [], message: "init", author: { name: "dev", email: "dev@example.com" } }));
  }

  putBlob(bytes) {
    const sha = sha1(Buffer.concat([Buffer.from(`blob ${bytes.length}\0`), bytes]));
    this.blobs.set(sha, bytes);
    return sha;
  }

  putTree(map) {
    const sorted = new Map([...map.entries()].sort(([a], [b]) => (a < b ? -1 : 1)));
    const sha = sha1(`tree ${JSON.stringify([...sorted])}`);
    this.trees.set(sha, sorted);
    return sha;
  }

  putCommit({ tree, parents, message, author }) {
    this.clock += 1000;
    const sha = sha1(`commit ${tree} ${parents.join(",")} ${message} ${JSON.stringify(author)} ${this.clock}`);
    this.commits.set(sha, { sha, tree, parents, message, author, date: new Date(this.clock).toISOString() });
    return sha;
  }

  /** Commits `changes` (path → text, or null to delete) straight to `branch` (a push that does
   * not go through the Worker). */
  push(branch, changes, message = "push", author = { name: "dev", email: "dev@example.com" }) {
    const head = this.refs.get(branch);
    const tree = new Map(this.trees.get(this.commits.get(head).tree));
    for (const [p, t] of Object.entries(changes)) {
      if (t === null) tree.delete(p);
      else tree.set(p, this.putBlob(Buffer.from(t)));
    }
    const sha = this.putCommit({ tree: this.putTree(tree), parents: [head], message, author });
    this.refs.set(branch, sha);
    return sha;
  }

  /** The text of `path` at `ref` (a branch or commit), or undefined. */
  text(ref, path) {
    const sha = this.refs.get(ref) ?? ref;
    const blob = this.trees.get(this.commits.get(sha)?.tree)?.get(path);
    return blob === undefined ? undefined : this.blobs.get(blob).toString("utf8");
  }

  head(branch) {
    return this.refs.get(branch);
  }

  commit(ref) {
    return this.commits.get(this.refs.get(ref) ?? ref);
  }

  ancestors(sha) {
    const seen = new Set();
    const queue = [sha];
    while (queue.length) {
      const s = queue.shift();
      if (seen.has(s)) continue;
      seen.add(s);
      queue.push(...this.commits.get(s).parents);
    }
    return seen;
  }

  mergeBase(a, b) {
    const ofA = this.ancestors(a);
    const queue = [b];
    const seen = new Set();
    while (queue.length) {
      const s = queue.shift();
      if (ofA.has(s)) return s;
      if (seen.has(s)) continue;
      seen.add(s);
      queue.push(...this.commits.get(s).parents);
    }
    return null;
  }

  /** Lets a GitHub App with `appId` and the public key `publicPem` mint tokens. */
  allowApp(appId, publicPem, installation = 77) {
    this.app = { appId: String(appId), publicPem, installation, issued: 0 };
  }

  checkAppJwt(auth) {
    const jwt = auth.replace(/^Bearer /, "");
    const [h, c, s] = jwt.split(".");
    const verify = createVerify("RSA-SHA256");
    verify.update(`${h}.${c}`);
    const ok = verify.verify(this.app.publicPem, Buffer.from(s, "base64url"));
    const claims = JSON.parse(Buffer.from(c, "base64url").toString());
    return ok && claims.iss === this.app.appId;
  }

  /** The `fetch` of the fake. */
  fetch = async (input, init = {}) => {
    const res = await this.respond(input, init);
    if (this.after) {
      const url = new URL(typeof input === "string" ? input : input.url);
      this.after((init.method ?? "GET").toUpperCase(), decodeURIComponent(url.pathname.replace(`/repos/${this.repo}`, "")));
    }
    return res;
  };

  async respond(input, init) {
    const url = new URL(typeof input === "string" ? input : input.url);
    const method = (init.method ?? "GET").toUpperCase();
    const auth = new Headers(init.headers).get("authorization") ?? "";
    const body = init.body ? JSON.parse(init.body) : undefined;
    this.calls.push(`${method} ${url.pathname}`);
    if (url.hostname !== "api.github.com") throw new Error(`unexpected fetch ${url}`);

    if (this.app && url.pathname === `/repos/${this.repo}/installation`) {
      return this.checkAppJwt(auth) ? jsonResponse(200, { id: this.app.installation }) : jsonResponse(401, { message: "Bad credentials" });
    }
    if (this.app && url.pathname === `/app/installations/${this.app.installation}/access_tokens`) {
      if (!this.checkAppJwt(auth)) return jsonResponse(401, { message: "Bad credentials" });
      if (JSON.stringify(body.permissions) !== '{"contents":"write"}') return jsonResponse(422, { message: "permissions" });
      this.app.issued++;
      return jsonResponse(201, { token: this.token, expires_at: new Date(Date.now() + 3600_000).toISOString() });
    }
    if (auth !== `Bearer ${this.token}`) return jsonResponse(401, { message: "Bad credentials" });
    if (url.pathname === "/graphql") return this.graphql(body);

    const prefix = `/repos/${this.repo}`;
    if (!url.pathname.startsWith(prefix)) return jsonResponse(404, { message: "Not Found" });
    const path = decodeURIComponent(url.pathname.slice(prefix.length));
    let m;
    if (method === "GET" && (m = path.match(/^\/git\/ref\/heads\/(.+)$/))) {
      const sha = this.refs.get(m[1]);
      return sha ? jsonResponse(200, { ref: `refs/heads/${m[1]}`, object: { sha } }) : jsonResponse(404, { message: "Not Found" });
    }
    if (method === "POST" && path === "/git/refs") {
      const name = body.ref.replace(/^refs\/heads\//, "");
      if (this.refs.has(name)) return jsonResponse(422, { message: "Reference already exists" });
      if (!this.commits.has(body.sha)) return jsonResponse(422, { message: "Object does not exist" });
      this.refs.set(name, body.sha);
      return jsonResponse(201, { ref: body.ref, object: { sha: body.sha } });
    }
    if (method === "PATCH" && (m = path.match(/^\/git\/refs\/heads\/(.+)$/))) {
      const current = this.refs.get(m[1]);
      if (!current) return jsonResponse(422, { message: "Reference does not exist" });
      if (!body.force && !this.ancestors(body.sha).has(current)) return jsonResponse(422, { message: "Update is not a fast forward" });
      this.refs.set(m[1], body.sha);
      return jsonResponse(200, { object: { sha: body.sha } });
    }
    if (method === "DELETE" && (m = path.match(/^\/git\/refs\/heads\/(.+)$/))) {
      if (!this.refs.delete(m[1])) return jsonResponse(422, { message: "Reference does not exist" });
      return jsonResponse(204);
    }
    if (method === "GET" && (m = path.match(/^\/git\/commits\/([0-9a-f]+)$/))) {
      const c = this.commits.get(m[1]);
      if (!c) return jsonResponse(404, { message: "Not Found" });
      return jsonResponse(200, { sha: c.sha, tree: { sha: c.tree }, message: c.message, author: c.author, parents: c.parents.map((sha) => ({ sha })) });
    }
    if (method === "POST" && path === "/git/blobs") {
      if (body.encoding !== "base64") return jsonResponse(422, { message: "encoding" });
      return jsonResponse(201, { sha: this.putBlob(Buffer.from(body.content, "base64")) });
    }
    if (method === "GET" && (m = path.match(/^\/git\/blobs\/([0-9a-f]+)$/))) {
      const b = this.blobs.get(m[1]);
      return b ? jsonResponse(200, { sha: m[1], content: b.toString("base64"), encoding: "base64" }) : jsonResponse(404, {});
    }
    if (method === "POST" && path === "/git/trees") {
      const base = this.trees.get(body.base_tree);
      if (!base) return jsonResponse(422, { message: "base_tree" });
      const tree = new Map(base);
      for (const e of body.tree) {
        if (e.mode !== "100644" || e.type !== "blob") return jsonResponse(422, { message: "mode" });
        if (e.content !== undefined) tree.set(e.path, this.putBlob(Buffer.from(e.content)));
        else if (e.sha === null) {
          if (!tree.delete(e.path)) return jsonResponse(422, { message: `GitRPC::BadObjectState ${e.path}` });
        } else if (this.blobs.has(e.sha)) tree.set(e.path, e.sha);
        else return jsonResponse(422, { message: "sha" });
      }
      return jsonResponse(201, { sha: this.putTree(tree) });
    }
    if (method === "POST" && path === "/git/commits") {
      if (!this.trees.has(body.tree) || !body.parents.every((p) => this.commits.has(p))) return jsonResponse(422, {});
      return jsonResponse(201, { sha: this.putCommit({ tree: body.tree, parents: body.parents, message: body.message, author: { name: body.author.name, email: body.author.email } }) });
    }
    if (method === "GET" && (m = path.match(/^\/contents\/(.+)$/))) {
      const ref = url.searchParams.get("ref");
      const sha = this.refs.get(ref) ?? ref;
      const commit = this.commits.get(sha);
      const blob = commit && this.trees.get(commit.tree).get(m[1]);
      if (!blob) return jsonResponse(404, { message: "Not Found" });
      const bytes = this.blobs.get(blob);
      const b64 = bytes.toString("base64").replace(/(.{60})/g, "$1\n");
      return jsonResponse(200, { type: "file", sha: blob, size: bytes.length, encoding: "base64", content: bytes.length > 1000 ? "" : b64 });
    }
    if (method === "GET" && (m = path.match(/^\/compare\/([0-9a-f]+)\.\.\.([0-9a-f]+)$/))) {
      const [base, head] = [m[1], m[2]];
      const mb = this.mergeBase(base, head);
      const from = this.trees.get(this.commits.get(mb).tree);
      const to = this.trees.get(this.commits.get(head).tree);
      const files = [];
      for (const p of new Set([...from.keys(), ...to.keys()])) {
        const a = from.get(p);
        const b = to.get(p);
        if (a === b) continue;
        const status = a === undefined ? "added" : b === undefined ? "removed" : "modified";
        const text = (s) => (s ? this.blobs.get(s).toString("utf8") : "");
        files.push({ filename: p, status, sha: b ?? a, patch: `@@ @@\n-${text(a)}\n+${text(b)}` });
      }
      const behind = this.ancestors(mb);
      const commits = [];
      for (let s = head; s && !behind.has(s); s = this.commits.get(s).parents[0]) {
        const c = this.commits.get(s);
        commits.unshift({ sha: s, commit: { author: c.author, message: c.message } });
      }
      return jsonResponse(200, { merge_base_commit: { sha: mb }, files: files.slice(0, this.compareLimit), commits });
    }
    return jsonResponse(404, { message: `fake: no ${method} ${path}` });
  }

  graphql({ query, variables }) {
    if (query.includes("refs(refPrefix")) {
      const prefix = variables.prefix.replace(/^refs\/heads\//, "");
      const nodes = [...this.refs.entries()]
        .filter(([name]) => name.startsWith(prefix))
        .map(([name, sha]) => {
          const c = this.commits.get(sha);
          return { name: name.slice(prefix.length), target: { oid: sha, message: c.message, committedDate: c.date, author: c.author } };
        })
        .sort((a, b) => (a.target.committedDate < b.target.committedDate ? 1 : -1));
      return jsonResponse(200, { data: { repository: { refs: { nodes } } } });
    }
    if (query.includes("object(expression")) {
      const repository = {};
      for (const [k, v] of Object.entries(variables)) {
        if (!/^e\d+$/.test(k)) continue;
        const [sha, ...rest] = v.split(":");
        const blob = this.trees.get(this.commits.get(sha)?.tree)?.get(rest.join(":"));
        repository[`f${k.slice(1)}`] = blob ? { oid: blob } : null;
      }
      return jsonResponse(200, { data: { repository } });
    }
    return jsonResponse(200, { errors: [{ message: "fake: unknown query" }] });
  }
}
