// The API (`api/`, next to the editor's page).

/** An error answer of the API, with its JSON body (`stale`, `conflicts`). */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly data: Record<string, unknown>,
  ) {
    super(message);
  }
}

/** The API's URL: `api/` in the editor's directory. */
const apiBase = () => new URL("api/", new URL(".", location.href));

export async function api<T>(method: "GET" | "POST", name: string, params?: Record<string, string | undefined> | null, body?: unknown): Promise<T> {
  const url = new URL(name, apiBase());
  for (const [k, v] of Object.entries(params ?? {})) if (v) url.searchParams.set(k, v);
  const res = await fetch(url, {
    method,
    credentials: "same-origin",
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  let data: Record<string, unknown> = {};
  try {
    data = await res.json();
  } catch {
    if (!res.ok) throw new ApiError(`the editor's API answered HTTP ${res.status}: is the Worker deployed?`, res.status, {});
  }
  if (!res.ok) throw new ApiError(typeof data.error === "string" ? data.error : `HTTP ${res.status}`, res.status, data);
  return data as T;
}

export const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** A way to sign in, as the API lists them when no one is signed in. */
export interface SignIn {
  provider: string;
  label: string;
}

/** The ways to sign in of an error, when it is the API's "not signed in". */
export function signInsOf(e: unknown): SignIn[] | null {
  const ways = e instanceof ApiError && e.status === 401 ? e.data.signIn : null;
  return Array.isArray(ways) ? ways : null;
}

/** The address that signs in with `provider`, then comes back to the route shown. */
export function signInUrl(provider: string): string {
  const url = new URL(`login/${encodeURIComponent(provider)}`, apiBase());
  if (location.hash) url.searchParams.set("to", location.hash);
  return url.href;
}

/** Signs out: the Worker's session, or Cloudflare Access's. */
export async function signOut(login: string): Promise<void> {
  if (login === "oauth") {
    await api("POST", "logout", null, {});
    location.reload();
  } else {
    location.assign("/cdn-cgi/access/logout");
  }
}
