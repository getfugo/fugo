// Signing in with a GitHub or Google account (`[cms.login] provider`), without Cloudflare Access.
//
// `login/<provider>` sends the browser to the provider (OAuth 2.0, with PKCE), with a state and
// a code verifier in a cookie of ten minutes (`__Host-cms-login`), which ties the sign-in to the
// browser that started it. The provider sends it back to `callback/<provider>` with a code; the
// Worker exchanges it for the account's verified emails (the provider's token is used for that
// and dropped), and the first of them that `CMS_USERS` gives a role signs in: a session cookie
// of seven days (`__Host-cms-session`), signed with `CMS_SESSION_KEY` (session.ts). Roles are
// looked up on every request, so taking someone out of `CMS_USERS` takes effect at once;
// changing `CMS_SESSION_KEY` signs everyone out.

import { cookieOf, seal, SessionKey, setCookie, unseal } from "./session";
import { b64urlJson, type Env, type Fetch, HttpError, type Json, json, type OauthProvider, type Settings, toB64url, USER_AGENT } from "./shared";

const SESSION = "__Host-cms-session";
const LOGIN = "__Host-cms-login";
/** How long a session lasts, and a sign-in may take, in seconds. */
const SESSION_AGE = 7 * 24 * 3600;
const LOGIN_AGE = 10 * 60;
/** A route of the editor to come back to (`#/e/…`). */
const ROUTE = /^#\/[!-~]{0,1000}$/;

/** An OAuth client: the provider's app for this editor. */
interface Client {
  id: string;
  secret: string;
}

/** The account a code signs in: its verified emails (lower case, the primary one first), and its
 * name. */
interface Account {
  emails: string[];
  name: string;
}

interface Provider {
  label: string;
  /** The secrets of the client's ID and secret. */
  secrets: [keyof Env, keyof Env];
  /** The provider's page that asks people to sign in, and what it is asked for. */
  authorize: string;
  params: Record<string, string>;
  account(code: string, client: Client, redirect: string, verifier: string, fetcher: Fetch, now: number): Promise<Account>;
}

const PROVIDERS: Record<OauthProvider, Provider> = {
  github: {
    label: "GitHub",
    secrets: ["CMS_GITHUB_CLIENT_ID", "CMS_GITHUB_CLIENT_SECRET"],
    authorize: "https://github.com/login/oauth/authorize",
    params: { scope: "user:email" },
    async account(code, client, redirect, verifier, fetcher) {
      const token = await post(fetcher, "https://github.com/login/oauth/access_token", client, { code, redirect_uri: redirect, code_verifier: verifier });
      if (typeof token.access_token !== "string") throw refused("GitHub", token);
      const get = async (path: string): Promise<Json> => {
        const res = await fetcher(`https://api.github.com${path}`, {
          headers: { authorization: `Bearer ${token.access_token}`, accept: "application/vnd.github+json", "x-github-api-version": "2022-11-28", "user-agent": USER_AGENT },
        });
        if (!res.ok) throw new HttpError(502, `GitHub: ${path}: HTTP ${res.status}`);
        return res.json();
      };
      const [emails, user] = await Promise.all([get("/user/emails"), get("/user")]);
      const verified = (Array.isArray(emails) ? emails : []).filter((e: Json) => e.verified === true && typeof e.email === "string");
      verified.sort((a: Json, b: Json) => Number(b.primary === true) - Number(a.primary === true));
      return { emails: verified.map((e: Json) => e.email.trim().toLowerCase()), name: String(user.name || user.login || "") };
    },
  },
  google: {
    label: "Google",
    secrets: ["CMS_GOOGLE_CLIENT_ID", "CMS_GOOGLE_CLIENT_SECRET"],
    authorize: "https://accounts.google.com/o/oauth2/v2/auth",
    params: { scope: "openid email profile", prompt: "select_account" },
    async account(code, client, redirect, verifier, fetcher, now) {
      const token = await post(fetcher, "https://oauth2.googleapis.com/token", client, {
        code,
        redirect_uri: redirect,
        code_verifier: verifier,
        grant_type: "authorization_code",
      });
      if (typeof token.id_token !== "string") throw refused("Google", token);
      // The ID token comes from Google itself, over TLS: its signature needs no check (OpenID
      // Connect Core 1.0, 3.1.3.7), its claims do.
      let claims: Json;
      try {
        claims = b64urlJson(token.id_token.split(".")[1]);
      } catch {
        throw new HttpError(502, "Google: the ID token is not a JWT");
      }
      const issuer = claims.iss === "https://accounts.google.com" || claims.iss === "accounts.google.com";
      if (!issuer || claims.aud !== client.id || !(claims.exp > now / 1000 - 60)) {
        throw new HttpError(502, "Google: the ID token is not one for this editor");
      }
      const verified = (claims.email_verified === true || claims.email_verified === "true") && typeof claims.email === "string";
      return { emails: verified ? [claims.email.trim().toLowerCase()] : [], name: String(claims.name ?? "") };
    },
  },
};

/** The sign-in of the providers of `[cms.login]`. */
export class OAuth {
  private readonly key = new SessionKey();

  constructor(
    private readonly settings: Settings,
    private readonly providers: OauthProvider[],
    private readonly fetcher: Fetch,
    private readonly now: () => number,
  ) {}

  /** `login/<provider>`: off to the provider, to come back to the editor's route `to`. */
  async start(url: URL, env: Env, name: string): Promise<Response> {
    try {
      const p = this.provider(name);
      const client = clientOf(env, p);
      const key = await this.key.key(env.CMS_SESSION_KEY);
      const state = random(16);
      const verifier = random(32);
      const challenge = toB64url(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));
      const to = url.searchParams.get("to") ?? "";
      const login = await seal(key, LOGIN, { p, s: state, v: verifier, to: ROUTE.test(to) ? to : "", x: this.now() + LOGIN_AGE * 1000 });
      const go = new URL(PROVIDERS[p].authorize);
      const params = { client_id: client.id, redirect_uri: this.redirect(url, p), response_type: "code", state, code_challenge: challenge, code_challenge_method: "S256" };
      for (const [k, v] of Object.entries({ ...params, ...PROVIDERS[p].params })) go.searchParams.set(k, v);
      return redirect(go.href, setCookie(LOGIN, login, LOGIN_AGE));
    } catch (e) {
      return this.failed(e);
    }
  }

  /** `callback/<provider>`: the provider's answer. The first verified email of the account that
   * has a role (`hasRole`) signs in. */
  async callback(request: Request, url: URL, env: Env, name: string, hasRole: (email: string) => boolean): Promise<Response> {
    try {
      const p = this.provider(name);
      const { label } = PROVIDERS[p];
      const key = await this.key.key(env.CMS_SESSION_KEY);
      const sealed = cookieOf(request, LOGIN);
      const login = sealed ? await unseal(key, LOGIN, sealed) : null;
      const again = "this sign-in was started in another browser, or took too long: sign in again";
      if (!login || login.p !== p || !(login.x > this.now())) throw new HttpError(400, again);
      const error = url.searchParams.get("error");
      if (error) throw new HttpError(403, error === "access_denied" ? `${label} sign-in cancelled` : `${label}: ${url.searchParams.get("error_description") || error}`);
      const code = url.searchParams.get("code");
      if (!code || url.searchParams.get("state") !== login.s) throw new HttpError(400, again);
      const account = await PROVIDERS[p].account(code, clientOf(env, p), this.redirect(url, p), login.v, this.fetcher, this.now());
      if (account.emails.length === 0) throw new HttpError(403, `the ${label} account has no verified email`);
      const email = account.emails.find(hasRole);
      if (!email) throw new HttpError(403, `${account.emails[0]} has no role in the editor`);
      const session = await seal(key, SESSION, { e: email, n: nameOf(account.name), x: this.now() + SESSION_AGE * 1000 });
      return redirect(`${this.settings.path}${login.to}`, setCookie(SESSION, session, SESSION_AGE), setCookie(LOGIN, "", 0));
    } catch (e) {
      return this.failed(e);
    }
  }

  /** Who the request's session cookie signs in; null without one. */
  async session(request: Request, env: Env): Promise<{ email: string; name: string } | null> {
    const sealed = cookieOf(request, SESSION);
    if (!sealed) return null;
    const s = await unseal(await this.key.key(env.CMS_SESSION_KEY), SESSION, sealed);
    if (!s || typeof s.e !== "string" || !(s.x > this.now())) throw this.notSignedIn("your sign-in has expired");
    return { email: s.e, name: typeof s.n === "string" ? s.n : "" };
  }

  /** `logout`: deletes the session cookie. */
  signOut(): Response {
    const res = json({ signedOut: true });
    res.headers.append("set-cookie", setCookie(SESSION, "", 0));
    return res;
  }

  /** A 401 with the ways to sign in, which the editor offers. */
  notSignedIn(why = "not signed in"): HttpError {
    const signIn = this.providers.map((p) => ({ provider: p, label: PROVIDERS[p].label }));
    return new HttpError(401, `${why}: sign in again (in another tab, to keep the changes of this one)`, { signIn });
  }

  private provider(name: string): OauthProvider {
    const p = this.providers.find((x) => x === name);
    if (!p) throw new HttpError(404, `no sign-in with ${name}`);
    return p;
  }

  /** The address the provider sends people back to: the one its app must allow. */
  private redirect(url: URL, p: OauthProvider): string {
    return `${url.origin}${this.settings.api}callback/${p}`;
  }

  /** A page that says why signing in failed (sign-in is a browser's navigation, not the
   * editor's call). */
  private failed(e: unknown): Response {
    const status = e instanceof HttpError ? e.status : 500;
    const message = e instanceof Error ? e.message : String(e);
    const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    const body = `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sign-in failed</title>
<p>Sign-in failed: ${escape(message)}.</p>
<p><a href="${escape(this.settings.path)}">Back to the editor</a></p>
</html>
`;
    const headers = new Headers({
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
      "content-security-policy": "default-src 'none'",
      "x-frame-options": "DENY",
      "x-content-type-options": "nosniff",
    });
    headers.append("set-cookie", setCookie(LOGIN, "", 0));
    return new Response(body, { status, headers });
  }
}

/** The OAuth client of a provider, from the Worker's secrets. */
function clientOf(env: Env, p: OauthProvider): Client {
  const [id, secret] = PROVIDERS[p].secrets;
  const values = [env[id], env[secret]];
  if (typeof values[0] !== "string" || !values[0] || typeof values[1] !== "string" || !values[1]) {
    throw new HttpError(500, `set the ${id} and ${secret} secrets (the ${PROVIDERS[p].label} OAuth app's client ID and secret)`);
  }
  return { id: values[0].trim(), secret: values[1].trim() };
}

/** A form POST to a provider's token endpoint, with the client's credentials. */
async function post(fetcher: Fetch, url: string, client: Client, form: Record<string, string>): Promise<Json> {
  const res = await fetcher(url, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/x-www-form-urlencoded", "user-agent": USER_AGENT },
    body: new URLSearchParams({ client_id: client.id, client_secret: client.secret, ...form }).toString(),
  });
  try {
    return await res.json();
  } catch {
    return { error: `HTTP ${res.status}` };
  }
}

const refused = (label: string, answer: Json) => new HttpError(502, `${label}: ${answer.error_description || answer.error || "no token"}`);

const random = (bytes: number) => toB64url(crypto.getRandomValues(new Uint8Array(bytes)));

/** A name for commits: no control characters or angle brackets (git's), not too long. */
const nameOf = (name: string) => name.replace(/[\u0000-\u001f\u007f<>]/g, "").trim().slice(0, 100);

const redirect = (location: string, ...cookies: string[]) => {
  const headers = new Headers({ location, "cache-control": "no-store" });
  for (const c of cookies) headers.append("set-cookie", c);
  return new Response(null, { status: 302, headers });
};
