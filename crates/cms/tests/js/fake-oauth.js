// A fake GitHub and Google to sign in with (OAuth 2.0 with PKCE): a test hands out a code for the
// address the Worker sent the browser to (`issue`), and the fake answers the Worker's exchange of
// it, at the token endpoints, and GitHub's API for the account.

import { createHash } from "node:crypto";

export const CLIENTS = {
  github: { id: "gh-client", secret: "gh-secret" },
  google: { id: "g-client.apps.googleusercontent.com", secret: "g-secret" },
};

const TOKEN_URLS = {
  "https://github.com/login/oauth/access_token": "github",
  "https://oauth2.googleapis.com/token": "google",
};

const b64url = (s) => Buffer.from(s).toString("base64url");

export class FakeOAuth {
  constructor(now) {
    this.now = now;
    /** code → what the provider's page was asked, and the account that signed in */
    this.codes = new Map();
    /** GitHub's tokens → accounts */
    this.tokens = new Map();
    this.calls = [];
  }

  /** The code the provider sends back after its page at `authorize` (the Worker's redirect):
   * GitHub `{ emails: [{ email, verified, primary }], name, login }`, Google `{ email,
   * email_verified, name }` (ID token claims). */
  issue(provider, authorize, account) {
    const u = new URL(authorize);
    const code = `code-${this.codes.size + 1}-${provider}`;
    this.codes.set(code, { provider, challenge: u.searchParams.get("code_challenge"), redirect: u.searchParams.get("redirect_uri"), account });
    return code;
  }

  handles(url) {
    return url in TOKEN_URLS || url === "https://api.github.com/user" || url === "https://api.github.com/user/emails";
  }

  async fetch(url, init = {}) {
    this.calls.push(url);
    const provider = TOKEN_URLS[url];
    if (provider) {
      const form = new URLSearchParams(init.body);
      const grant = this.codes.get(form.get("code"));
      this.codes.delete(form.get("code"));
      const client = CLIENTS[provider];
      const ok =
        grant?.provider === provider &&
        form.get("client_id") === client.id &&
        form.get("client_secret") === client.secret &&
        form.get("redirect_uri") === grant.redirect &&
        createHash("sha256").update(form.get("code_verifier") ?? "").digest("base64url") === grant.challenge &&
        (provider === "github" || form.get("grant_type") === "authorization_code");
      if (!ok) {
        // GitHub answers 200 with an error; Google 400.
        return provider === "github"
          ? Response.json({ error: "bad_verification_code", error_description: "The code passed is incorrect or expired." })
          : Response.json({ error: "invalid_grant", error_description: "Bad Request" }, { status: 400 });
      }
      if (provider === "github") {
        const token = `gho_${this.tokens.size + 1}`;
        this.tokens.set(token, grant.account);
        return Response.json({ access_token: token, token_type: "bearer", scope: "user:email" });
      }
      const claims = { iss: "https://accounts.google.com", aud: client.id, sub: "1", exp: this.now() / 1000 + 3600, ...grant.account };
      return Response.json({ access_token: "ya29.x", id_token: `${b64url('{"alg":"RS256"}')}.${b64url(JSON.stringify(claims))}.sig` });
    }
    const auth = new Headers(init.headers).get("authorization") ?? "";
    const account = this.tokens.get(auth.replace(/^Bearer /, ""));
    if (!account) return Response.json({ message: "Bad credentials" }, { status: 401 });
    if (url.endsWith("/emails")) return Response.json(account.emails);
    return Response.json({ login: account.login ?? "octocat", name: account.name ?? null });
  }
}
