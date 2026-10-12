import { apiFetch, apiPost, clearCsrf, getCsrf, refreshCsrf } from './client';
import { http, HttpResponse, server } from '../test/server';

describe('CSRF header (doc 05 section 2)', () => {
  beforeEach(() => clearCsrf());

  it('sends X-SR-CSRF on mutations, fetched once from /sr/csrf', async () => {
    let csrfCalls = 0;
    const seen: (string | null)[] = [];
    server.use(
      http.get('*/api/v1/sr/csrf', () => {
        csrfCalls += 1;
        return HttpResponse.json({ token: 'tok-1' });
      }),
      http.post('*/api/v1/sr/thing', ({ request }) => {
        seen.push(request.headers.get('x-sr-csrf'));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await apiPost('/sr/thing');
    await apiPost('/sr/thing');
    expect(seen).toEqual(['tok-1', 'tok-1']);
    expect(csrfCalls).toBe(1);
  });

  it('does not add the header to GET requests', async () => {
    let header: string | null = 'unset';
    server.use(
      http.get('*/api/v1/sr/thing', ({ request }) => {
        header = request.headers.get('x-sr-csrf');
        return HttpResponse.json({});
      }),
    );
    await apiFetch('/sr/thing');
    expect(header).toBeNull();
  });

  it('sends pre-login mutations without a token when there is no session', async () => {
    let header: string | null = 'unset';
    server.use(
      http.get('*/api/v1/sr/csrf', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
      http.post('*/api/v1/sr/auth/login', ({ request }) => {
        header = request.headers.get('x-sr-csrf');
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    await apiPost('/sr/auth/login', { login: 'a', password: 'b' });
    expect(header).toBeNull();
  });

  it('refreshes once and retries when the server rejects the token', async () => {
    let issued = 0;
    const seen: (string | null)[] = [];
    server.use(
      http.get('*/api/v1/sr/csrf', () => {
        issued += 1;
        return HttpResponse.json({ token: `tok-${issued}` });
      }),
      http.post('*/api/v1/sr/thing', ({ request }) => {
        const h = request.headers.get('x-sr-csrf');
        seen.push(h);
        if (h !== 'tok-2')
          return HttpResponse.json({ message: 'Missing or invalid CSRF token' }, { status: 403 });
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await expect(apiPost('/sr/thing')).resolves.toBeUndefined();
    expect(seen).toEqual(['tok-1', 'tok-2']);
  });

  it('gives up after one retry', async () => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'x' })),
      http.post('*/api/v1/sr/thing', () =>
        HttpResponse.json({ message: 'Missing or invalid CSRF token' }, { status: 403 }),
      ),
    );
    await expect(apiPost('/sr/thing')).rejects.toMatchObject({ status: 403 });
  });

  it('refreshCsrf reloads the token after login', async () => {
    let issued = 0;
    server.use(
      http.get('*/api/v1/sr/csrf', () => {
        issued += 1;
        return HttpResponse.json({ token: `t${issued}` });
      }),
    );
    expect(await getCsrf()).toBe('t1');
    expect(await refreshCsrf()).toBe('t2');
    expect(await getCsrf()).toBe('t2');
  });
});
