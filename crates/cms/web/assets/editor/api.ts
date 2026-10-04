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
