// GitHub App tokens: an installation token from the app's private key (PKCS#1 or PKCS#8), signed as
// a JWT.

import { type Fetch, GitError, type Json, USER_AGENT } from "./shared";

export type TokenCache = Map<string, { token: string; expires: number }>;

/** An installation token of the GitHub App for `repo`, cached until 5 minutes before it
 * expires. */
export async function appToken(
  cache: TokenCache,
  appId: string,
  pem: string,
  repo: string,
  fetcher: Fetch,
  now: () => number,
): Promise<string> {
  const cached = cache.get(repo);
  if (cached && cached.expires - now() > 5 * 60_000) return cached.token;
  const key = await importAppKey(pem);
  const iat = Math.floor(now() / 1000) - 60;
  const jwt = await signJwt({ alg: "RS256", typ: "JWT" }, { iat, exp: iat + 540, iss: String(appId).trim() }, key);
  const call = async (method: string, path: string, body?: unknown): Promise<Json> => {
    const res = await fetcher(`https://api.github.com${path}`, {
      method,
      headers: {
        authorization: `Bearer ${jwt}`,
        accept: "application/vnd.github+json",
        "x-github-api-version": "2022-11-28",
        "user-agent": USER_AGENT,
        ...(body ? { "content-type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      try {
        message = ((await res.json()) as Json).message ?? message;
      } catch {}
      throw new GitError("GitHub App", res.status, `${message} (is the app installed on ${repo}?)`);
    }
    return res.json();
  };
  const installation = await call("GET", `/repos/${repo}/installation`);
  const token = await call("POST", `/app/installations/${installation.id}/access_tokens`, {
    repositories: [repo.split("/")[1]],
    permissions: { contents: "write" },
  });
  cache.set(repo, { token: token.token, expires: Date.parse(token.expires_at) });
  return token.token;
}

/** An RSA private key from PEM: PKCS#8 (`BEGIN PRIVATE KEY`) or PKCS#1 (`BEGIN RSA PRIVATE KEY`,
 * what GitHub hands out). Literal `\n` in the secret count as line breaks. */
async function importAppKey(pem: string): Promise<CryptoKey> {
  const text = pem.replace(/\\n/g, "\n");
  const body = text.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "");
  const der = Uint8Array.from(atob(body), (c) => c.charCodeAt(0));
  const pkcs8 = /BEGIN RSA PRIVATE KEY/.test(text) ? pkcs1ToPkcs8(der) : der;
  return crypto.subtle.importKey("pkcs8", pkcs8, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["sign"]);
}

function derLength(n: number): number[] {
  if (n < 0x80) return [n];
  const bytes: number[] = [];
  for (; n > 0; n >>= 8) bytes.unshift(n & 0xff);
  return [0x80 | bytes.length, ...bytes];
}

/** Wraps a PKCS#1 RSAPrivateKey in a PKCS#8 PrivateKeyInfo (rsaEncryption, NULL parameters). */
export function pkcs1ToPkcs8(pkcs1: Uint8Array): Uint8Array<ArrayBuffer> {
  const algorithm = [0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00];
  const inner = [0x02, 0x01, 0x00, ...algorithm, 0x04, ...derLength(pkcs1.length)];
  const head = [0x30, ...derLength(inner.length + pkcs1.length), ...inner];
  const out = new Uint8Array(head.length + pkcs1.length);
  out.set(head, 0);
  out.set(pkcs1, head.length);
  return out;
}

async function signJwt(header: object, claims: object, key: CryptoKey): Promise<string> {
  const enc = (o: object) => btoa(JSON.stringify(o)).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_");
  const input = `${enc(header)}.${enc(claims)}`;
  const sig = new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(input)));
  let s = "";
  for (const b of sig) s += String.fromCharCode(b);
  return `${input}.${btoa(s).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_")}`;
}
