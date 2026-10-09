// Signed cookies: the session of someone the Worker signed in, and the state of a sign-in under
// way. A cookie holds its value as JSON in base64url, then an HMAC-SHA-256 of the cookie's name
// and that text, keyed with the CMS_SESSION_KEY secret: readable, but not forgeable, and one
// cookie's value is not valid as another's.

import { b64urlBytes, b64urlJson, HttpError, type Json, toB64url } from "./shared";

const utf8 = (s: string) => new TextEncoder().encode(s);

/** The HMAC key of the CMS_SESSION_KEY secret, imported once for each value of it. */
export class SessionKey {
  private cached: { raw: string; key: Promise<CryptoKey> } | null = null;

  key(raw: string | undefined): Promise<CryptoKey> {
    if (!raw) throw new HttpError(500, "the CMS_SESSION_KEY secret is not set");
    if (raw.length < 32) throw new HttpError(500, "the CMS_SESSION_KEY secret is too short: give 32 random characters or more");
    if (this.cached?.raw !== raw) {
      const key = crypto.subtle.importKey("raw", utf8(raw), { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);
      this.cached = { raw, key };
    }
    return this.cached.key;
  }
}

/** `value` signed as the value of the cookie `name`. */
export async function seal(key: CryptoKey, name: string, value: Json): Promise<string> {
  const body = toB64url(utf8(JSON.stringify(value)));
  const mac = await crypto.subtle.sign("HMAC", key, utf8(`${name}=${body}`));
  return `${body}.${toB64url(new Uint8Array(mac))}`;
}

/** The value `seal` signed for the cookie `name`; null for anything else. */
export async function unseal(key: CryptoKey, name: string, text: string): Promise<Json | null> {
  const [body, mac, more] = text.split(".");
  if (!body || !mac || more !== undefined) return null;
  try {
    if (!(await crypto.subtle.verify("HMAC", key, b64urlBytes(mac), utf8(`${name}=${body}`)))) return null;
    return b64urlJson(body);
  } catch {
    return null;
  }
}

/** The cookie `name` of a request. */
export function cookieOf(request: Request, name: string): string | null {
  for (const part of (request.headers.get("cookie") ?? "").split(";")) {
    const at = part.indexOf("=");
    if (at > 0 && part.slice(0, at).trim() === name) return part.slice(at + 1).trim();
  }
  return null;
}

/** A `set-cookie` value: a cookie of this host only (`__Host-` names need `Path=/` and `Secure`),
 * sent over HTTPS only, that scripts cannot read, and sent from other sites only on links (the
 * provider's way back is one). `maxAge` 0 deletes it. */
export const setCookie = (name: string, value: string, maxAge: number) =>
  `${name}=${value}; Path=/; Max-Age=${maxAge}; HttpOnly; Secure; SameSite=Lax`;
