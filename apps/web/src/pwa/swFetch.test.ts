// RDY-PWA-001: the service worker's fetch, activate and message handlers, run against an in-memory cache. The policy
// (`classify`) is tested in pwa.test.tsx; this is what the worker does with a request once it has taken it.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SOURCE = readFileSync(resolve(__dirname, '../../public/sw.js'), 'utf8');
const ORIGIN = 'https://side.example';
const SURFACES = [
  { shell: 'index.html', paths: ['/', '/join/$code'] },
  { shell: 'auth.html', paths: ['/login', '/reset/$token'] },
  { shell: null, paths: ['/house-notes'] },
];

type Listener = (event: Record<string, unknown>) => void;

interface Harness {
  /** Cache name -> request URL path -> response. */
  caches: Map<string, Map<string, Response>>;
  skipped: () => boolean;
  claimed: () => boolean;
  /** Dispatch a request and resolve what the worker answered, or `undefined` if it did not respond. */
  fetchEvent: (request: unknown) => Promise<Response | undefined>;
  install: () => Promise<void>;
  activate: () => Promise<void>;
  /** Post a message to the worker; resolves the replies it sent back on the message port. */
  message: (data: unknown) => Promise<unknown[]>;
  network: { calls: string[] };
}

/** Boot sw.js with the build id `build`; `online` decides what `fetch` does. */
function boot(
  build: string,
  online: (url: string) => Response | Promise<Response>,
  existing = new Map<string, Map<string, Response>>(),
): Harness {
  const listeners: Record<string, Listener> = {};
  const store = existing;
  const path = (r: RequestInfo | URL | { url: string }) =>
    new URL(
      typeof r === 'string' ? r : r instanceof URL ? r.href : (r as { url: string }).url,
      ORIGIN,
    ).pathname;
  const cacheOf = (name: string) => {
    if (!store.has(name)) store.set(name, new Map());
    const m = store.get(name)!;
    return {
      add: async (r: RequestInfo | { url: string }) => {
        const res = await fromNetwork(typeof r === 'string' ? r : (r as { url: string }).url);
        m.set(path(r), res);
      },
      put: async (r: RequestInfo | { url: string }, res: Response) => void m.set(path(r), res),
      match: async (r: RequestInfo | { url: string }) => m.get(path(r))?.clone(),
      delete: async (r: RequestInfo | { url: string }) => m.delete(path(r)),
    };
  };
  const cachesApi = {
    open: async (name: string) => cacheOf(name),
    keys: async () => [...store.keys()],
    delete: async (name: string) => store.delete(name),
  };
  let skipped = false;
  let claimed = false;
  const network = { calls: [] as string[] };
  // A same-origin response is "basic" in a browser; Node's Response says "default".
  const fromNetwork = async (url: string) => {
    const res = await online(url);
    // A test may have set its own `type` (an unfollowed redirect): keep it.
    if (Object.prototype.hasOwnProperty.call(res, 'type')) return res;
    return Object.defineProperty(res, 'type', { value: 'basic' });
  };
  const fetchApi = async (r: { url: string }) => {
    network.calls.push(path(r));
    return fromNetwork(r.url);
  };
  const self = {
    addEventListener: (type: string, fn: Listener) => void (listeners[type] = fn),
    location: { origin: ORIGIN },
    skipWaiting: () => void (skipped = true),
    clients: { claim: async () => void (claimed = true) },
  };
  // The build step replaces the two placeholders; do the same.
  const src = SOURCE.replace('__SR_BUILD__', build)
    .replace('/*__SR_PRECACHE__*/ []', JSON.stringify(['/entrance/landing-a.js']))
    .replace('/*__SR_SURFACES__*/ []', JSON.stringify(SURFACES));
  class FakeRequest {
    url: string;
    constructor(u: string) {
      this.url = ORIGIN + u;
    }
  }
  new Function('module', 'self', 'caches', 'fetch', 'Request', src)(
    { exports: {} },
    self,
    cachesApi,
    fetchApi,
    FakeRequest,
  );

  return {
    caches: store,
    skipped: () => skipped,
    claimed: () => claimed,
    network,
    fetchEvent: async (request) => {
      let answer: Promise<Response> | undefined;
      const pending: Promise<unknown>[] = [];
      listeners.fetch!({
        request,
        respondWith: (p: Promise<Response>) => void (answer = p),
        waitUntil: (p: Promise<unknown>) => void pending.push(p),
      });
      if (!answer) return undefined;
      const res = await answer;
      // What the worker keeps doing after it answered (storing the shell) is part of the behaviour.
      await Promise.all(pending);
      return res;
    },
    install: async () => {
      let done: Promise<unknown> = Promise.resolve();
      listeners.install!({ waitUntil: (p: Promise<unknown>) => void (done = p) });
      await done;
    },
    activate: async () => {
      let done: Promise<unknown> = Promise.resolve();
      listeners.activate!({ waitUntil: (p: Promise<unknown>) => void (done = p) });
      await done;
    },
    message: async (data) => {
      const replies: unknown[] = [];
      let done: Promise<unknown> = Promise.resolve();
      listeners.message!({
        data,
        ports: [{ postMessage: (m: unknown) => void replies.push(m) }],
        waitUntil: (p: Promise<unknown>) => void (done = p),
      });
      await done;
      return replies;
    },
  };
}

const navigate = (p: string) => req(p, 'navigate');
function req(p: string, mode: string, method = 'GET') {
  return {
    url: ORIGIN + p,
    method,
    mode,
    headers: { get: () => null, has: () => false },
  };
}

const html = (body: string, init: ResponseInit = {}) =>
  new Response(body, { status: 200, headers: { 'content-type': 'text/html' }, ...init });

const SHELL_HTML =
  '<!doctype html><div id="root"></div><script src="/entrance/landing-a.js"></script>';
const APP_HTML = '<!doctype html><div id="root"></div><script src="/app/main-a.js"></script>';
const OFFLINE_HTML = '<!doctype html><h1>You are offline</h1>';

const site = (url: string): Response => {
  const p = new URL(url).pathname;
  if (p === '/offline.html') return html(OFFLINE_HTML);
  if (p.startsWith('/entrance/') || p.startsWith('/app/')) {
    return new Response('code', { headers: { 'content-type': 'text/javascript' } });
  }
  if (p === '/index.html' || p === '/auth.html') return html(SHELL_HTML);
  if (p === '/' || p === '/login') return html(SHELL_HTML);
  return html(APP_HTML);
};

const down = () => Promise.reject(new TypeError('Failed to fetch'));

const storeOf = (...caches: Array<[string, Array<[string, Response]>]>) =>
  new Map(caches.map(([name, rows]) => [name, new Map(rows)]));

describe('install', () => {
  it('precaches the public shells, the plain pages and the public assets, and nothing signed-in', async () => {
    const w = boot('b1', site);
    await w.install();
    expect([...w.caches.keys()]).toEqual(['sr-static-b1']);
    expect([...w.caches.get('sr-static-b1')!.keys()].sort()).toEqual([
      '/404.html',
      '/50x.html',
      '/auth.html',
      '/entrance/landing-a.js',
      '/index.html',
      '/offline.html',
    ]);
    expect(w.network.calls.some((p) => p.startsWith('/app'))).toBe(false);
  });
});

describe('the shell caches', () => {
  it('a public page is stored as its surface shell in the static cache only', async () => {
    const w = boot('b1', site);
    await (await w.fetchEvent(navigate('/login')))!.text();
    expect([...w.caches.keys()]).toEqual(['sr-static-b1']);
    expect(await w.caches.get('sr-static-b1')!.get('/auth.html')!.clone().text()).toBe(SHELL_HTML);
    await (await w.fetchEvent(navigate('/join/abc')))!.text();
    expect(w.caches.get('sr-static-b1')!.has('/index.html')).toBe(true);
  });

  it('any other page is stored as the signed-in shell in the app cache only', async () => {
    const w = boot('b1', site);
    await (await w.fetchEvent(navigate('/channels/1/2')))!.text();
    expect([...w.caches.keys()]).toEqual(['sr-app-b1']);
    expect(await w.caches.get('sr-app-b1')!.get('/app.html')!.clone().text()).toBe(APP_HTML);
  });

  it('an anonymous session that only opens public pages never creates an app cache', async () => {
    const w = boot('b1', site);
    for (const p of ['/', '/login', '/join/x', '/reset/t', '/house-notes'])
      await (await w.fetchEvent(navigate(p)))!.text();
    await w.fetchEvent(req('/entrance/landing-a.js', 'no-cors'));
    expect([...w.caches.keys()].filter((k) => k.startsWith('sr-app-'))).toEqual([]);
  });

  it('a plain public page (no script, no root) is shown but never stored as a shell', async () => {
    const w = boot('b1', () => html('<h1>House notes</h1>'));
    expect(await (await w.fetchEvent(navigate('/house-notes')))!.text()).toContain('House notes');
    expect([...w.caches.keys()].flatMap((k) => [...w.caches.get(k)!.keys()])).toEqual([]);
  });

  it('a navigation to the offline page itself is left to the network and never replaces a shell', async () => {
    const w = boot('b1', site);
    await (await w.fetchEvent(navigate('/login')))!.text();
    expect(await w.fetchEvent(navigate('/offline.html'))).toBeUndefined();
    expect(await w.caches.get('sr-static-b1')!.get('/auth.html')!.clone().text()).toBe(SHELL_HTML);
  });

  it('an html page that is not ours (a captive portal, an error page) is not stored as a shell', async () => {
    const good = storeOf(['sr-static-b1', [['/auth.html', html(SHELL_HTML)]]]);
    const w = boot('b1', () => html('<h1>Log in to the wifi</h1>'), good);
    const res = await (await w.fetchEvent(navigate('/login')))!.text();
    expect(res).toContain('wifi'); // online: what the network said is shown
    expect(await good.get('sr-static-b1')!.get('/auth.html')!.clone().text()).toBe(SHELL_HTML);
  });

  it('offline, a public page gets its surface shell, any other page the signed-in shell, else the offline page', async () => {
    const store = storeOf(
      [
        'sr-static-b1',
        [
          ['/auth.html', html(SHELL_HTML)],
          ['/offline.html', html(OFFLINE_HTML)],
        ],
      ],
      ['sr-app-b1', [['/app.html', html(APP_HTML)]]],
    );
    const w = boot('b1', down, store);
    expect(await (await w.fetchEvent(navigate('/reset/abc')))!.text()).toBe(SHELL_HTML);
    expect(await (await w.fetchEvent(navigate('/channels/1/2')))!.text()).toBe(APP_HTML);
    // No cached shell for that surface, and a plain page has none at all: the offline page.
    expect(await (await w.fetchEvent(navigate('/join/x')))!.text()).toBe(OFFLINE_HTML);
    expect(await (await w.fetchEvent(navigate('/house-notes')))!.text()).toBe(OFFLINE_HTML);

    store.delete('sr-app-b1'); // signed out: the purge took the signed-in shell
    expect(await (await w.fetchEvent(navigate('/channels/1/2')))!.text()).toBe(OFFLINE_HTML);
  });

  it('a redirect or an error is not stored as a shell', async () => {
    const store = storeOf(['sr-static-b1', [['/auth.html', html(SHELL_HTML)]]]);
    const redirected = Object.defineProperty(html('<div id="root">moved</div>'), 'redirected', {
      value: true,
    });
    const w = boot('b1', () => redirected, store);
    await (await w.fetchEvent(navigate('/login')))!.text();
    expect(await store.get('sr-static-b1')!.get('/auth.html')!.clone().text()).toBe(SHELL_HTML);
    const w2 = boot('b1', () => html('<div id="root">oops</div>', { status: 500 }), store);
    await (await w2.fetchEvent(navigate('/login')))!.text();
    expect(await store.get('sr-static-b1')!.get('/auth.html')!.clone().text()).toBe(SHELL_HTML);
  });
});

describe('signed-in files', () => {
  const appRows = (): Array<[string, Array<[string, Response]>]> => [
    ['sr-app-b1', [['/app.html', html(APP_HTML)]]],
    ['sr-app-b0', [['/app/old.js', new Response('old')]]],
    ['sr-static-b1', [['/auth.html', html(SHELL_HTML)]]],
  ];
  const appKeys = (store: Map<string, unknown>) =>
    [...store.keys()].filter((k) => k.startsWith('sr-app-'));

  it('are cached on first use in the app cache, cache-first, and never in the static one', async () => {
    const w = boot('b1', site);
    expect(await (await w.fetchEvent(req('/app/chunk-x.js', 'no-cors')))!.text()).toBe('code');
    await w.fetchEvent(req('/app/chunk-x.js', 'no-cors'));
    expect(w.network.calls).toEqual(['/app/chunk-x.js']);
    expect([...w.caches.keys()]).toEqual(['sr-app-b1']);
  });

  it('a 401 on one purges every app cache and keeps the static one', async () => {
    const store = storeOf(...appRows());
    const w = boot('b1', () => new Response('', { status: 401 }), store);
    const res = await w.fetchEvent(req('/app/chunk-y.js', 'no-cors'));
    expect(res!.status).toBe(401);
    expect(appKeys(store)).toEqual([]);
    expect(store.has('sr-static-b1')).toBe(true);
  });

  it('a request that was redirected to the sign-in page purges them too and is not stored', async () => {
    const store = storeOf(...appRows());
    const login = Object.defineProperties(html('<div id="root"></div>'), {
      redirected: { value: true },
      url: { value: ORIGIN + '/login?next=%2Fapp' },
    });
    const w = boot('b1', () => login, store);
    await w.fetchEvent(req('/app/chunk-z.js', 'no-cors'));
    expect(appKeys(store)).toEqual([]);
  });

  it('a page navigation answered with an unfollowed redirect or a 401 purges them', async () => {
    for (const make of [
      () => Object.defineProperty(new Response(null), 'type', { value: 'opaqueredirect' }),
      () => new Response('', { status: 401 }),
    ]) {
      const store = storeOf(...appRows());
      const w = boot('b1', make, store);
      await w.fetchEvent(navigate('/channels'));
      expect(appKeys(store)).toEqual([]);
    }
  });

  it('a 401 on a public page does not touch them', async () => {
    const store = storeOf(...appRows());
    const w = boot('b1', () => new Response('', { status: 401 }), store);
    await w.fetchEvent(navigate('/login'));
    expect(appKeys(store).sort()).toEqual(['sr-app-b0', 'sr-app-b1']);
  });

  it('PURGE_APP deletes every app cache, answers PURGED, and leaves the static cache', async () => {
    const store = storeOf(...appRows());
    const w = boot('b1', site, store);
    expect(await w.message({ type: 'PURGE_APP' })).toEqual([{ type: 'PURGED' }]);
    expect(appKeys(store)).toEqual([]);
    expect(store.has('sr-static-b1')).toBe(true);
  });
});

describe('updates', () => {
  it('SKIP_WAITING activates the waiting worker, other messages do nothing', async () => {
    const w = boot('b2', site);
    await w.message({ type: 'HELLO' });
    await w.message(undefined);
    expect(w.skipped()).toBe(false);
    await w.message({ type: 'SKIP_WAITING' });
    expect(w.skipped()).toBe(true);
  });

  it('activating removes older static caches and every app cache, and nothing else, then takes control', async () => {
    const old = storeOf(
      ['sr-static-b1', []],
      ['sr-static-b2', []],
      ['sr-app-b1', []],
      ['sr-app-b2', []],
      ['sr-reader-offline', []],
    );
    const w = boot('b2', site, old);
    await w.activate();
    expect([...old.keys()].sort()).toEqual(['sr-reader-offline', 'sr-static-b2']);
    expect(w.claimed()).toBe(true);
  });

  it('public assets are served from the cache of their own build and cached on first use', async () => {
    const w = boot('b2', site);
    const first = await w.fetchEvent(req('/entrance/lazy-x.js', 'no-cors'));
    expect(await first!.text()).toBe('code');
    expect(w.network.calls).toEqual(['/entrance/lazy-x.js']);
    await w.fetchEvent(req('/entrance/lazy-x.js', 'no-cors'));
    expect(w.network.calls).toEqual(['/entrance/lazy-x.js']);
  });

  it('API, gateway and media requests are never taken', async () => {
    const w = boot('b2', site);
    for (const p of ['/api/v1/users/@me', '/gateway', '/cdn/a/b.png']) {
      expect(await w.fetchEvent(req(p, 'cors'))).toBeUndefined();
    }
    expect(await w.fetchEvent(req('/api/v1/x', 'cors', 'POST'))).toBeUndefined();
  });
});
