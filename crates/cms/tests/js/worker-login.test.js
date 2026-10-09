// Signing in with GitHub and Google (`[cms.login] provider`), against fake providers: the way
// there and back, the session cookie, signing out, and what is refused.

import { test } from "node:test";
import assert from "node:assert/strict";

import { draftId } from "../../assets/worker.js";
import { CLIENTS } from "./fake-oauth.js";
import { NOW, OAUTH_SETTINGS, ORIGIN, setup } from "./worker-setup.js";

const API = `${ORIGIN}/admin/api/`;
const DAY = 24 * 3600_000;
const COOKIE_ATTRS = ["Path=/", "HttpOnly", "Secure", "SameSite=Lax"];

/** A GitHub account: a primary email without a role, an unverified one with a role, and a
 * verified one with a role. */
const GITHUB = {
  emails: [
    { email: "Me@Personal.example", verified: true, primary: true },
    { email: "boss@example.com", verified: false, primary: false },
    { email: "Writer@Example.com", verified: true, primary: false },
  ],
  name: "Ann <Writer>\n",
  login: "ann",
};

/** The cookies a response sets, by name. */
function setCookies(res) {
  const out = new Map();
  for (const c of res.headers.getSetCookie()) {
    const [pair, ...attrs] = c.split(";").map((s) => s.trim());
    const at = pair.indexOf("=");
    out.set(pair.slice(0, at), { value: pair.slice(at + 1), attrs });
  }
  return out;
}

/** Starts signing in with `provider`, as the editor's link does. */
async function start(w, provider, to = "#/e/blog%2Fpost") {
  const res = await w.worker.fetch(new Request(`${API}login/${provider}?to=${encodeURIComponent(to)}`), w.env);
  return { res, go: res.headers.get("location"), login: setCookies(res).get("__Host-cms-login")?.value };
}

/** Signs in with `provider` as `account`: the provider's way back, and the session cookie. */
async function signIn(w, provider, account, { to } = {}) {
  const { go, login } = await start(w, provider, to);
  const code = w.oauth.issue(provider, go, account);
  const state = new URL(go).searchParams.get("state");
  const back = await w.worker.fetch(
    new Request(`${API}callback/${provider}?code=${code}&state=${state}`, { headers: { cookie: `__Host-cms-login=${login}` } }),
    w.env,
  );
  return { back, session: setCookies(back).get("__Host-cms-session")?.value };
}

/** The options of `call` for a request with a session cookie (and no Access token). */
const withSession = (session, headers = {}) => ({ jwt: null, headers: { cookie: `theme=dark; __Host-cms-session=${session}`, ...headers } });

const WAYS = [
  { provider: "github", label: "GitHub" },
  { provider: "google", label: "Google" },
];

test("GitHub: off to GitHub with PKCE, back with a session for the first verified email with a role", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const { res, go, login } = await start(w, "github");
  assert.equal(res.status, 302);
  assert.equal(res.headers.get("cache-control"), "no-store");
  const u = new URL(go);
  assert.equal(`${u.origin}${u.pathname}`, "https://github.com/login/oauth/authorize");
  assert.equal(u.searchParams.get("client_id"), CLIENTS.github.id);
  assert.equal(u.searchParams.get("redirect_uri"), `${API}callback/github`);
  assert.equal(u.searchParams.get("scope"), "user:email");
  assert.equal(u.searchParams.get("code_challenge_method"), "S256");
  assert.match(u.searchParams.get("state"), /^[\w-]{22}$/);
  assert.ok(login);
  assert.deepEqual(setCookies(res).get("__Host-cms-login").attrs.sort(), [...COOKIE_ATTRS, "Max-Age=600"].sort());

  const { back, session } = await signIn(w, "github", GITHUB);
  assert.equal(back.status, 302);
  assert.equal(back.headers.get("location"), "/admin/#/e/blog%2Fpost");
  const cookies = setCookies(back);
  assert.deepEqual(cookies.get("__Host-cms-session").attrs.sort(), [...COOKIE_ATTRS, "Max-Age=604800"].sort());
  assert.equal(cookies.get("__Host-cms-login").value, "", "the sign-in's cookie is deleted");
  assert.ok(cookies.get("__Host-cms-login").attrs.includes("Max-Age=0"));
  assert.ok(!session.includes("gho_"), "GitHub's token is not kept");

  const me = await w.call("GET", "me", withSession(session));
  assert.equal(me.status, 200, JSON.stringify(me.body));
  assert.equal(me.body.email, "writer@example.com", "the primary email has no role, the one with a role is not verified");
  assert.equal(me.body.login, "oauth");

  // Commits carry the account's name (without what git does not allow in one).
  const path = "content/blog/post.md";
  const file = await w.call("GET", "file", { ...withSession(session), query: { path } });
  const saved = await w.call("POST", "save", {
    ...withSession(session),
    body: { entry: "blog/post", title: "", changes: [{ path, content: "---\ntitle: Post 2\n---\n", base: file.body.sha }] },
  });
  assert.equal(saved.status, 200, JSON.stringify(saved.body));
  assert.deepEqual(w.gh.commit(`cms/${await draftId("blog/post")}`).author, { name: "Ann Writer", email: "writer@example.com" });
});

test("Google: a verified email signs in; an unverified one does not", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const u = new URL((await start(w, "google")).go);
  assert.equal(`${u.origin}${u.pathname}`, "https://accounts.google.com/o/oauth2/v2/auth");
  assert.equal(u.searchParams.get("client_id"), CLIENTS.google.id);
  assert.equal(u.searchParams.get("redirect_uri"), `${API}callback/google`);
  assert.equal(u.searchParams.get("response_type"), "code");
  assert.equal(u.searchParams.get("scope"), "openid email profile");

  const { back, session } = await signIn(w, "google", { email: "Boss@Example.com", email_verified: true, name: "The Boss" });
  assert.equal(back.status, 302);
  const me = await w.call("GET", "me", withSession(session));
  assert.equal(me.body.email, "boss@example.com");
  assert.equal(me.body.publish, true);

  const unverified = await signIn(w, "google", { email: "boss@example.com", email_verified: false });
  assert.equal(unverified.back.status, 403);
  assert.equal(unverified.session, undefined);
  assert.match(await unverified.back.text(), /the Google account has no verified email/);
});

test("an account without a role gets no session", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const account = { emails: [{ email: "stranger@example.com", verified: true, primary: true }, { email: "ghost@example.com", verified: true }] };
  const { back, session } = await signIn(w, "github", account);
  assert.equal(back.status, 403);
  assert.equal(session, undefined);
  assert.match(await back.text(), /stranger@example\.com has no role in the editor/);
});

test("the way back is refused without the browser's sign-in, with another state, late, or cancelled", async () => {
  let now = NOW;
  const w = setup({ settings: OAUTH_SETTINGS, now: () => now });
  const back = (provider, query, login) =>
    w.worker.fetch(
      new Request(`${API}callback/${provider}?${new URLSearchParams(query)}`, { headers: login ? { cookie: `__Host-cms-login=${login}` } : {} }),
      w.env,
    );
  const { go, login } = await start(w, "github");
  const state = new URL(go).searchParams.get("state");
  const code = () => w.oauth.issue("github", go, GITHUB);
  const again = /started in another browser, or took too long/;
  const cases = [
    // Someone else's sign-in, sent to this browser (login CSRF): no cookie of its own.
    [await back("github", { code: code(), state }), 400, again],
    [await back("github", { code: code(), state }, `${login.slice(0, -2)}xx`), 400, again],
    [await back("github", { code: code(), state: "other" }, login), 400, again],
    [await back("google", { code: code(), state }, login), 400, again],
    [await back("github", { error: "access_denied", state }, login), 403, /GitHub sign-in cancelled/],
    [await back("github", { error: "x", error_description: "<b>no</b>", state }, login), 403, /GitHub: &lt;b&gt;no&lt;\/b&gt;/],
    [await back("github", { code: "not-issued", state }, login), 502, /GitHub: The code passed is incorrect or expired/],
  ];
  now += 11 * 60_000;
  cases.push([await back("github", { code: code(), state }, login), 400, again]);
  for (const [res, status, message] of cases) {
    assert.equal(res.status, status);
    assert.equal(res.headers.get("content-type"), "text/html; charset=utf-8");
    assert.equal(res.headers.get("content-security-policy"), "default-src 'none'");
    assert.match(await res.text(), message);
    assert.equal(setCookies(res).get("__Host-cms-session"), undefined);
  }
});

test("sessions: a changed, foreign or old cookie is not a sign-in, and the API lists the ways to sign in", async () => {
  let now = NOW;
  const w = setup({ settings: OAUTH_SETTINGS, now: () => now });
  const { session } = await signIn(w, "github", GITHUB);
  const { login } = await start(w, "github");

  const none = await w.call("GET", "site", { jwt: null });
  assert.equal(none.status, 401);
  assert.match(none.body.error, /^not signed in/);
  assert.deepEqual(none.body.signIn, WAYS);

  const [body, mac] = session.split(".");
  const forged = Buffer.from(JSON.stringify({ e: "boss@example.com", n: "", x: NOW + DAY })).toString("base64url");
  for (const cookie of [`${forged}.${mac}`, `${body}.${mac.slice(2)}`, login, "x"]) {
    const r = await w.call("GET", "me", withSession(cookie));
    assert.equal(r.status, 401, cookie);
    assert.match(r.body.error, /^your sign-in has expired/);
    assert.deepEqual(r.body.signIn, WAYS);
  }
  const rotated = await w.worker.fetch(new Request(`${API}me`, { headers: { cookie: `__Host-cms-session=${session}` } }), {
    ...w.env,
    CMS_SESSION_KEY: "another session key of thirty-two characters",
  });
  assert.equal(rotated.status, 401, "a new key signs everyone out");

  assert.equal((await w.call("GET", "me", withSession(session))).status, 200);
  now += 7 * DAY + 1000;
  assert.equal((await w.call("GET", "me", withSession(session))).status, 401, "a session lasts seven days");
});

test("roles are looked up on every request; signing out deletes the session", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const { session } = await signIn(w, "google", { email: "boss@example.com", email_verified: true });
  const env = { ...w.env, CMS_USERS: JSON.stringify({ "writer@example.com": ["writer"] }) };
  const removed = await w.worker.fetch(new Request(`${API}me`, { headers: { cookie: `__Host-cms-session=${session}` } }), env);
  assert.equal(removed.status, 403);

  const crossSite = await w.call("POST", "logout", { ...withSession(session, { origin: "https://evil.example" }), body: {} });
  assert.equal(crossSite.status, 403);
  const out = await w.call("POST", "logout", { ...withSession(session), body: {} });
  assert.equal(out.status, 200);
  const cookie = setCookies(out).get("__Host-cms-session");
  assert.equal(cookie.value, "");
  assert.ok(cookie.attrs.includes("Max-Age=0"));
});

test("signing in needs its secrets, and a provider of the settings", async () => {
  const w = setup({ settings: { ...OAUTH_SETTINGS, login: { kind: "oauth", providers: ["google"] } } });
  const login = (name, env = {}) => w.worker.fetch(new Request(`${API}login/${name}`), { ...w.env, ...env });
  const github = await login("github");
  assert.equal(github.status, 404);
  assert.match(await github.text(), /no sign-in with github/);
  const cases = [
    [{ CMS_SESSION_KEY: "" }, /the CMS_SESSION_KEY secret is not set/],
    [{ CMS_SESSION_KEY: "short" }, /too short/],
    [{ CMS_GOOGLE_CLIENT_SECRET: "" }, /set the CMS_GOOGLE_CLIENT_ID and CMS_GOOGLE_CLIENT_SECRET secrets/],
    [{ CMS_USERS: "" }, null],
  ];
  for (const [env, message] of cases) {
    const r = await login("google", env);
    if (message === null) {
      assert.equal(r.status, 302, "CMS_USERS is read on the way back");
      continue;
    }
    assert.equal(r.status, 500);
    assert.match(await r.text(), message);
  }
  const anonymous = await w.call("GET", "site", { jwt: null });
  assert.deepEqual(anonymous.body.signIn, [{ provider: "google", label: "Google" }]);
});

test("the way back leads to a route of the editor only", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const cases = [
    ["#/drafts", "/admin/#/drafts"],
    ["", "/admin/"],
    ["//evil.example/", "/admin/"],
    ["https://evil.example/#/", "/admin/"],
    ["#/x\r\nset-cookie: a=b", "/admin/"],
  ];
  for (const [to, location] of cases) {
    const { back } = await signIn(w, "github", GITHUB, { to });
    assert.equal(back.headers.get("location"), location, JSON.stringify(to));
  }
});

test("CMS_DEV_USER signs in requests to localhost without a session", async () => {
  const w = setup({ settings: OAUTH_SETTINGS });
  const env = { ...w.env, CMS_DEV_USER: "boss@example.com" };
  const local = await w.worker.fetch(new Request("http://localhost:8787/admin/api/me"), env);
  assert.equal(local.status, 200);
  assert.equal((await local.json()).email, "boss@example.com");
  const deployed = await w.worker.fetch(new Request(`${API}me`), env);
  assert.equal(deployed.status, 401);
});

test("with Cloudflare Access, the Worker does not sign people in", async () => {
  const w = setup();
  assert.equal((await w.call("GET", "login/github")).status, 404);
  assert.equal((await w.call("GET", "me")).body.login, "cloudflare-access");
  const anonymous = await w.call("GET", "me", { jwt: null, headers: { cookie: "__Host-cms-session=x.y" } });
  assert.equal(anonymous.status, 401);
  assert.match(anonymous.body.error, /Cloudflare Access/);
  assert.equal(anonymous.body.signIn, undefined);
});
