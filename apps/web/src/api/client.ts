// FND-API-005 stub: fetch wrapper with 429 retry and CSRF. Replaced by the generated client later.
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
  /** A raw body (a file); sent as is with the type the file carries. Not retried on a CSRF refresh with `json`. */
  raw?: Blob | ArrayBuffer | null;
  /** 429 auto-retries. Auth endpoints pass 0 so lockouts surface to the user. */
  retries?: number;
  /** Internal: set once a rejected CSRF token has been refreshed and the request resent. */
  csrfRetried?: boolean;
}

/**
 * CSRF token for cookie sessions (doc 05 section 2). Fetched lazily from `GET /sr/csrf` the first time a
 * mutation needs it, kept in memory, and refreshed after login/register (`refreshCsrf`). Anonymous callers
 * get `null`, which is cached until the next refresh, so pre-login POSTs send no header (the server only
 * checks it for session principals).
 */
let csrf: { token: string | null; loaded: boolean; pending: Promise<string | null> | null } = {
  token: null,
  loaded: false,
  pending: null,
};

async function fetchCsrf(): Promise<string | null> {
  try {
    const res = await fetch('/api/v1/sr/csrf', { credentials: 'include' });
    if (!res.ok) return null;
    const body = (await res.json()) as { token?: unknown };
    return typeof body.token === 'string' ? body.token : null;
  } catch {
    return null;
  }
}

/** The cached CSRF token, loading it once when needed. */
export function getCsrf(): Promise<string | null> {
  if (csrf.loaded) return Promise.resolve(csrf.token);
  if (!csrf.pending) {
    csrf.pending = fetchCsrf().then((token) => {
      csrf = { token, loaded: true, pending: null };
      return token;
    });
  }
  return csrf.pending;
}

/** Re-reads the token; call after login, register and 2FA so the new session's token is used. */
export function refreshCsrf(): Promise<string | null> {
  csrf = { token: null, loaded: false, pending: null };
  return getCsrf();
}

/** Forgets the token (after logout or when the session is gone). */
export function clearCsrf(): void {
  csrf = { token: null, loaded: false, pending: null };
}

const isCsrfRejection = (e: ApiError) => e.status === 403 && /csrf/i.test(e.body?.message ?? '');

export async function apiFetch<T>(path: string, opts: ApiOptions = {}): Promise<T> {
  const { json, raw, retries = 2, headers, csrfRetried = false, ...init } = opts;
  const method = (init.method ?? 'GET').toUpperCase();
  const mutating = method !== 'GET' && method !== 'HEAD';
  const h = new Headers(headers);
  if (json !== undefined) h.set('content-type', 'application/json');
  if (mutating) {
    const token = await getCsrf();
    if (token) h.set('X-SR-CSRF', token);
  }
  const res = await fetch(`/api/v1${path}`, {
    credentials: 'include',
    ...init,
    headers: h,
    body: json === undefined ? (raw ?? undefined) : JSON.stringify(json),
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
    const error = new ApiError(res.status, body?.message ?? res.statusText, body);
    // A stale token (session rotated elsewhere): refetch once and retry the mutation.
    if (mutating && !csrfRetried && isCsrfRejection(error)) {
      await refreshCsrf();
      return apiFetch<T>(path, { ...opts, csrfRetried: true });
    }
    throw error;
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const apiPost = <T>(path: string, json?: unknown, opts: ApiOptions = {}) =>
  apiFetch<T>(path, { retries: 0, ...opts, method: 'POST', json: json ?? {} });
