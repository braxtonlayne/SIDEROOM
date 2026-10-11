// FND-API-005 stub: fetch wrapper with 429 retry. Replaced by the generated client later.
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function apiFetch<T>(path: string, init?: RequestInit, retries = 2): Promise<T> {
  const res = await fetch(`/api/v1${path}`, { credentials: 'include', ...init });
  if (res.status === 429 && retries > 0) {
    const wait = Number(res.headers.get('retry-after') ?? '1') * 1000;
    await new Promise((r) => setTimeout(r, wait));
    return apiFetch<T>(path, init, retries - 1);
  }
  if (!res.ok) throw new ApiError(res.status, res.statusText);
  return (await res.json()) as T;
}
