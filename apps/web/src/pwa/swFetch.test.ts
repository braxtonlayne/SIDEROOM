// RDY-PWA-001: the service worker's fetch, activate and message handlers, run against an in-memory cache. The policy
// (`classify`) is tested in pwa.test.tsx; this is what the worker does with a request once it has taken it.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SOURCE = readFileSync(resolve(__dirname, '../../public/sw.js'), 'utf8');
const ORIGIN = 'https://side.example';

type Listener = (event: Record<string, unknown>) => void;

interface Harness {
  /** Cache name -> request URL path -> response. */
  caches: Map<string, Map<string, Response>>;
  skipped: () => boolean;
  claimed: () => boolean;
  /** Dispatch a request and resolve what the worker answered, or `undefined` if it did not respond. */
  fetchEvent: (request: unknown) => Promise<Response | undefined>;
  activate: () => Promise<void>;
  message: (data: unknown) => void;
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
  const src = SOURCE.replace('__SR_BUILD__', build).replace(
    '/*__SR_PRECACHE__*/ []',
    JSON.stringify(['/assets/index-a.js']),
  );
  new Function('module', 'self', 'caches', 'fetch', src)(
    { exports: {} },
    self,
    cachesApi,
    fetchApi,
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
    activate: async () => {
      let done: Promise<unknown> = Promise.resolve();
      listeners.activate!({ waitUntil: (p: Promise<unknown>) => void (done = p) });
      await done;
    },
    message: (data) => listeners.message!({ data }),
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

const SHELL_HTML = '<!doctype html><div id="root"></div><script src="/assets/index-a.js"></script>';
const OFFLINE_HTML = '<!doctype html><h1>You are offline</h1>';

const site = (url: string): Response => {
  const p = new URL(url).pathname;
  if (p === '/offline.html') return html(OFFLINE_HTML);
  if (p.startsWith('/assets/')) {
    return new Response('code', { headers: { 'content-type': 'text/javascript' } });
  }
  return html(SHELL_HTML);
};

const down = () => Promise.reject(new TypeError('Failed to fetch'));

describe('the shell cache', () => {
  it('a navigation to the offline page itself never replaces the app shell', async () => {
    const w = boot('b1', site);
    const stored = () => w.caches.get('sr-static-b1')!.get('/index.html')!.clone().text();
    await (await w.fetchEvent(navigate('/login')))!.text(); // a navigation stores the shell
    expect(await stored()).toBe(SHELL_HTML);
    await (await w.fetchEvent(navigate('/offline.html')))!.text();
    // Whatever the worker did with that navigation, the stored shell is still the app.
    expect(await stored()).toBe(SHELL_HTML);
  });

  it('an html page that is not the app (a captive portal, an error page) is not stored as the shell', async () => {
    const good = new Map<string, Map<string, Response>>([
      ['sr-static-b1', new Map([['/index.html', html(SHELL_HTML)]])],
    ]);
    let portal = true;
    const w = boot('b1', (u) => (portal ? html('<h1>Log in to the wifi</h1>') : site(u)), good);
    const res = await (await w.fetchEvent(navigate('/channels')))!.text();
    expect(res).toContain('wifi'); // online: what the network said is shown
    expect(await good.get('sr-static-b1')!.get('/index.html')!.clone().text()).toBe(SHELL_HTML);
    portal = false;
  });

  it('offline, a navigation gets the stored shell, and the offline page when there is none', async () => {
    const withShell = boot(
      'b1',
      down,
      new Map([['sr-static-b1', new Map([['/index.html', html(SHELL_HTML)]])]]),
    );
    expect(await (await withShell.fetchEvent(navigate('/channels/1/2')))!.text()).toBe(SHELL_HTML);

    const noShell = boot(
      'b1',
      down,
      new Map([['sr-static-b1', new Map([['/offline.html', html(OFFLINE_HTML)]])]]),
    );
    expect(await (await noShell.fetchEvent(navigate('/channels/1/2')))!.text()).toBe(OFFLINE_HTML);
    // The offline page can itself be opened (its "Try again" link points home, but the address may be typed).
    expect(await (await noShell.fetchEvent(navigate('/offline.html')))!.text()).toBe(OFFLINE_HTML);
  });

  it('a redirect or an error is not stored as the shell', async () => {
    const store = new Map([['sr-static-b1', new Map([['/index.html', html(SHELL_HTML)]])]]);
    const redirected = Object.defineProperty(html('<div id="root">moved</div>'), 'redirected', {
      value: true,
    });
    const w = boot('b1', () => redirected, store);
    await (await w.fetchEvent(navigate('/')))!.text();
    expect(await store.get('sr-static-b1')!.get('/index.html')!.clone().text()).toBe(SHELL_HTML);
    const w2 = boot('b1', () => html('<div id="root">oops</div>', { status: 500 }), store);
    await (await w2.fetchEvent(navigate('/')))!.text();
    expect(await store.get('sr-static-b1')!.get('/index.html')!.clone().text()).toBe(SHELL_HTML);
  });
});

describe('updates', () => {
  it('SKIP_WAITING activates the waiting worker, other messages do nothing', () => {
    const w = boot('b2', site);
    w.message({ type: 'HELLO' });
    w.message(undefined);
    expect(w.skipped()).toBe(false);
    w.message({ type: 'SKIP_WAITING' });
    expect(w.skipped()).toBe(true);
  });

  it('activating removes the caches of older builds and nothing else, then takes control', async () => {
    const old = new Map([
      ['sr-static-b1', new Map<string, Response>()],
      ['sr-static-b2', new Map<string, Response>()],
      ['sr-reader-offline', new Map<string, Response>()],
    ]);
    const w = boot('b2', site, old);
    await w.activate();
    expect([...old.keys()].sort()).toEqual(['sr-reader-offline', 'sr-static-b2']);
    expect(w.claimed()).toBe(true);
  });

  it('assets are served from the cache of their own build and cached on first use', async () => {
    const w = boot('b2', site);
    const first = await w.fetchEvent(req('/assets/lazy-x.js', 'no-cors'));
    expect(await first!.text()).toBe('code');
    expect(w.network.calls).toEqual(['/assets/lazy-x.js']);
    await w.fetchEvent(req('/assets/lazy-x.js', 'no-cors'));
    expect(w.network.calls).toEqual(['/assets/lazy-x.js']);
  });

  it('API, gateway and media requests are never taken', async () => {
    const w = boot('b2', site);
    for (const p of ['/api/v1/users/@me', '/gateway', '/cdn/a/b.png']) {
      expect(await w.fetchEvent(req(p, 'cors'))).toBeUndefined();
    }
    expect(await w.fetchEvent(req('/api/v1/x', 'cors', 'POST'))).toBeUndefined();
  });
});
