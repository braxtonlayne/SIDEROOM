// FND-API-005 stub: fetch wrapper with 429 retry. Replaced by the generated client later.
export interface ApiErrorBody {
  code?: number;
  message?: string;
  retry_after?: number;
  /** Discord-style field error tree: `{ field: { _errors: [{ message }] } }`. */
  errors?: Record<string, { _errors?: { code?: string; message: string }[] }>;
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    /** Parsed JSON error body, when there was one. */
    public readonly body: ApiErrorBody | null = null,
  ) {
    super(message);
  }

  /** Seconds the server asked us to wait (429), if known. */
  get retryAfter(): number | null {
    return typeof this.body?.retry_after === 'number' ? this.body.retry_after : null;
  }
}

export interface ApiOptions extends Omit<RequestInit, 'body'> {
  /** JSON-serialisable request body. */
  json?: unknown;
  /** 429 auto-retries. Auth endpoints pass 0 so lockouts surface to the user. */
  retries?: number;
}

export async function apiFetch<T>(path: string, opts: ApiOptions = {}): Promise<T> {
  const { json, retries = 2, headers, ...init } = opts;
  const h = new Headers(headers);
  if (json !== undefined) h.set('content-type', 'application/json');
  const res = await fetch(`/api/v1${path}`, {
    credentials: 'include',
    ...init,
    headers: h,
    body: json === undefined ? undefined : JSON.stringify(json),
  });
  if (res.status === 429 && retries > 0) {
    const wait = Number(res.headers.get('retry-after') ?? '1') * 1000;
    await new Promise((r) => setTimeout(r, wait));
    return apiFetch<T>(path, { ...opts, retries: retries - 1 });
  }
  if (!res.ok) {
    let body: ApiErrorBody | null = null;
    try {
      body = (await res.json()) as ApiErrorBody;
    } catch {
      /* no JSON body */
    }
    if (res.status === 429) {
      const header = Number(res.headers.get('retry-after'));
      if (body && body.retry_after === undefined && Number.isFinite(header))
        body.retry_after = header;
    }
    throw new ApiError(res.status, body?.message ?? res.statusText, body);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const apiPost = <T>(path: string, json?: unknown, opts: ApiOptions = {}) =>
  apiFetch<T>(path, { retries: 0, ...opts, method: 'POST', json: json ?? {} });
