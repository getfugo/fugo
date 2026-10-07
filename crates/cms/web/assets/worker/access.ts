// Cloudflare Access: the public keys of a team, and the tokens Access adds to requests.

import { type Fetch, HttpError, type Json, type Settings } from "./shared";

const b64url = (s: string) => s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=");
const b64urlBytes = (s: string) => Uint8Array.from(atob(b64url(s)), (c) => c.charCodeAt(0));
const b64urlJson = (s: string): Json => JSON.parse(new TextDecoder().decode(b64urlBytes(s)));

/** The public keys of an Access team, fetched when a token names a key not seen yet (at most
 * once a minute; requests that come while they are fetched wait for them). */
export class AccessKeys {
  private keys = new Map<string, CryptoKey>();
  private fetchedAt = -Infinity;
  private fetching: Promise<void> | null = null;

  constructor(
    private readonly login: Settings["login"],
    private readonly fetcher: Fetch,
    private readonly now: () => number,
  ) {}

  /** The email of a valid token (lower case). */
  async verify(token: string): Promise<string> {
    const invalid = (why: string) => new HttpError(401, `invalid sign-in token: ${why}`);
    const parts = token.split(".");
    if (parts.length !== 3) throw invalid("not a JWT");
    let header: Json;
    let claims: Json;
    try {
      header = b64urlJson(parts[0]);
      claims = b64urlJson(parts[1]);
    } catch {
      throw invalid("not a JWT");
    }
    if (header.alg !== "RS256") throw invalid(`algorithm ${header.alg}`);
    const key = await this.key(header.kid);
    const signed = new TextEncoder().encode(`${parts[0]}.${parts[1]}`);
    let ok = false;
    try {
      ok = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, b64urlBytes(parts[2]), signed);
    } catch {
      ok = false;
    }
    if (!ok) throw invalid("bad signature");
    if (claims.iss !== this.login.team) throw invalid("issued by another team");
    const aud: unknown[] = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
    if (!aud.some((a) => typeof a === "string" && this.login.aud.includes(a))) {
      throw invalid("for another application");
    }
    const t = this.now() / 1000;
    if (typeof claims.exp !== "number" || claims.exp < t - 60) throw invalid("expired");
    if (typeof claims.nbf === "number" && claims.nbf > t + 60) throw invalid("not valid yet");
    if (typeof claims.email !== "string" || !claims.email.includes("@")) {
      throw new HttpError(403, "this sign-in has no email (service tokens cannot edit)");
    }
    return claims.email.trim().toLowerCase();
  }

  private async key(kid: unknown): Promise<CryptoKey> {
    const unknownKey = () => new HttpError(401, "invalid sign-in token: unknown key");
    if (typeof kid !== "string") throw unknownKey();
    // A fresh Worker gets the editor's requests at once (a page's files): they wait for the
    // one fetch.
    if (!this.keys.has(kid) && this.fetching) await this.fetching;
    const known = this.keys.get(kid);
    if (known) return known;
    if (this.now() - this.fetchedAt < 60_000) throw unknownKey();
    this.fetchedAt = this.now();
    this.fetching = this.fetchKeys().finally(() => {
      this.fetching = null;
    });
    await this.fetching;
    const found = this.keys.get(kid);
    if (!found) throw unknownKey();
    return found;
  }

  /** Fetches the team's keys, in place of those known. */
  private async fetchKeys(): Promise<void> {
    const res = await this.fetcher(`${this.login.team}/cdn-cgi/access/certs`);
    if (!res.ok) throw new HttpError(502, `Cloudflare Access keys: HTTP ${res.status}`);
    const body: Json = await res.json();
    const keys = new Map<string, CryptoKey>();
    for (const jwk of body.keys ?? []) {
      if (jwk.kty !== "RSA" || !jwk.kid) continue;
      const key = await crypto.subtle.importKey(
        "jwk",
        { kty: "RSA", n: jwk.n, e: jwk.e, alg: "RS256", ext: true },
        { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
        false,
        ["verify"],
      );
      keys.set(jwk.kid, key);
    }
    this.keys = keys;
  }
}
