/*
 * SIDE ROOM service worker (XC-PWA-002, SEC-ENT-002).
 *
 * A plain classic script: copied verbatim from public/, then `pwaPlugin()` replaces the three placeholders below
 * with the build id, the list of public files to precache and the table of public surfaces. It is registered in
 * production builds only (src/pwa/register.ts).
 *
 * Two caches, one rule: the worker knows the public pages and nothing else by name.
 *   - sr-static-<build>: the public shells, the public assets they reference, the offline page. Precached.
 *   - sr-app-<build>: the signed-in files, cache-first on first use, never precached, never requested by the
 *     worker on its own. Deleted on PURGE_APP, on a 401 or a redirect to the sign-in page, and on activation.
 *   - Navigations: network first. A public path falls back to its surface's shell, any other path to the cached
 *     signed-in shell; both then to the offline page.
 *   - Everything else (non-GET, other origins, any path that is neither a public file nor a navigation,
 *     Authorization and Range requests) is bypassed by default: the browser talks to the network directly.
 *   - Updates: a new worker waits; the page shows "A new version is ready" and posts SKIP_WAITING.
 */
const BUILD = '__SR_BUILD__';
const PRECACHE = /*__SR_PRECACHE__*/ [];
// Public surfaces: [{ shell: 'index.html' | null, paths: ['/', '/join/$code'] }], from public-surfaces.json.
const SURFACES = /*__SR_SURFACES__*/ [];
const STATIC_CACHE = 'sr-static-' + BUILD;
const APP_CACHE = 'sr-app-' + BUILD;
const APP_SHELL = '/app.html';
const APP_FILES = '/app/';
const OFFLINE = '/offline.html';
const PLAIN_PAGES = [OFFLINE, '/404.html', '/50x.html'];
// What a shell contains and no other page of this origin does: the element the page mounts into.
const SHELL_MARK = 'id="root"';
const STATIC_PREFIXES = ['/assets/', '/fonts/', '/icons/', '/entrance/'];
const STATIC_FILES = ['/manifest.webmanifest', '/offline.css', ...PLAIN_PAGES];

/** Does the surface path pattern ('$x' marks one variable segment, '~$x' a prefixed one) match this path? */
function pathMatches(pattern, path) {
  const clean = pattern.split('#')[0];
  if (clean === '/') return path === '/';
  const ps = clean.split('/').slice(1);
  const xs = path.split('/').slice(1);
  if (xs.length > 1 && xs[xs.length - 1] === '') xs.pop();
  if (ps.length !== xs.length) return false;
  return ps.every((seg, i) => {
    const at = seg.indexOf('$');
    if (at < 0) return seg === xs[i];
    return xs[i].length > at && xs[i].startsWith(seg.slice(0, at));
  });
}

/** The public surface a path belongs to, or null. */
function surfaceFor(path) {
  return SURFACES.find((s) => s.paths.some((p) => pathMatches(p, path))) || null;
}

/** The last path segment names a file ('/x/y.png'): not a page, so never answered with a shell. */
function looksLikeFile(path) {
  const last = path.slice(path.lastIndexOf('/') + 1);
  return last.includes('.');
}

/**
 * 'bypass' (do not intercept), 'share' (Share Target POST), 'navigate-public' (a public surface's page),
 * 'navigate-app' (any other page), 'static' (immutable public build output) or 'app-file' (signed-in build output).
 */
function classify(request, origin) {
  const url = new URL(request.url);
  if (request.method === 'POST' && url.origin === origin && url.pathname === '/share')
    return 'share';
  if (request.method !== 'GET') return 'bypass';
  if (url.origin !== origin) return 'bypass';
  const p = url.pathname;
  if (request.headers.get('authorization') || request.headers.has('range')) return 'bypass';
  if (p.startsWith(APP_FILES) && request.mode !== 'navigate') return 'app-file';
  if (request.mode === 'navigate') {
    // The plain pages are static files: opening one must neither count as a shell (it would be stored in the
    // shell's place) nor be answered with a shell when offline (RDY-PWA-001).
    if (PLAIN_PAGES.includes(p)) return 'bypass';
    if (surfaceFor(p)) return 'navigate-public';
    if (looksLikeFile(p)) return 'bypass';
    return 'navigate-app';
  }
  if (STATIC_PREFIXES.some((x) => p.startsWith(x)) || STATIC_FILES.includes(p)) return 'static';
  return 'bypass';
}

/**
 * Share Target (G-18): the manifest posts shared files as multipart/form-data. Files go to IndexedDB (same
 * names as src/pwa/shareFiles.ts); the page reads them after the redirect. Text fields travel in the query
 * string like the GET form, so SharePage parses both the same way.
 */
function stashShare(form) {
  const files = form.getAll('files').filter((f) => typeof f !== 'string');
  const q = new URLSearchParams();
  for (const k of ['title', 'text', 'url']) {
    const v = form.get(k);
    if (typeof v === 'string' && v) q.set(k, v);
  }
  if (files.length === 0) return Promise.resolve(q);
  return new Promise((resolve) => {
    const req = indexedDB.open('sr-share', 1);
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains('files')) req.result.createObjectStore('files');
    };
    req.onerror = () => resolve(q);
    req.onsuccess = () => {
      const db = req.result;
      const tx = db.transaction('files', 'readwrite');
      const store = tx.objectStore('files');
      store.clear();
      files.forEach((f, i) => store.put(f, i));
      tx.oncomplete = () => {
        db.close();
        q.set('files', '1');
        resolve(q);
      };
      tx.onerror = tx.onabort = () => {
        db.close();
        resolve(q);
      };
    };
  });
}

/** Delete every signed-in cache, whatever its build. */
function purgeApp() {
  return caches
    .keys()
    .then((keys) =>
      Promise.all(keys.filter((k) => k.startsWith('sr-app-')).map((k) => caches.delete(k))),
    )
    .catch(() => undefined);
}

/** The server refused the session: a 401, a redirect that was followed to the sign-in page, or one left unfollowed. */
function sessionLost(res) {
  if (res.status === 401 || res.type === 'opaqueredirect') return true;
  if (res.redirected) {
    try {
      return new URL(res.url).pathname === '/login';
    } catch {
      return false;
    }
  }
  return false;
}

/** A page of ours worth keeping as a shell: a plain 200 html document. */
function storableShell(res) {
  const ct = res.headers.get('content-type') || '';
  return res.ok && !res.redirected && ct.includes('text/html');
}

/** Network first for a page; remember it in `cacheName` under `key` when it is a shell; fall back to the cache. */
function navigateWith(event, cacheName, key) {
  return fetch(event.request)
    .then((res) => {
      if (cacheName === APP_CACHE && sessionLost(res)) {
        event.waitUntil(purgeApp());
        return res;
      }
      if (key && storableShell(res)) {
        // Store it only if it is a shell: a captive portal or a proxy's page is html too, and would otherwise
        // be what every offline navigation shows from then on (RDY-PWA-001).
        const probe = res.clone();
        const copy = res.clone();
        event.waitUntil(
          probe
            .text()
            .then((html) =>
              html.includes(SHELL_MARK)
                ? caches.open(cacheName).then((c) => c.put(key, copy))
                : undefined,
            )
            .catch(() => undefined),
        );
      }
      return res;
    })
    .catch(() =>
      caches
        .open(cacheName)
        .then((c) => (key ? c.match(key) : undefined))
        .then((hit) => hit || caches.open(STATIC_CACHE).then((c) => c.match(OFFLINE))),
    );
}

if (typeof self !== 'undefined' && typeof self.addEventListener === 'function') {
  self.addEventListener('install', (event) => {
    const shells = SURFACES.filter((s) => s.shell).map((s) => '/' + s.shell);
    event.waitUntil(
      caches.open(STATIC_CACHE).then((cache) =>
        // `reload` bypasses the HTTP cache so the shells and assets match this build. Public files only.
        Promise.all(
          [...new Set([...shells, ...PLAIN_PAGES, ...PRECACHE])].map((u) =>
            cache.add(new Request(u, { cache: 'reload' })).catch(() => undefined),
          ),
        ),
      ),
    );
  });

  self.addEventListener('activate', (event) => {
    event.waitUntil(
      caches
        .keys()
        .then((keys) =>
          Promise.all(
            keys
              // A new build starts with no signed-in files at all: they are fetched again, behind the session.
              .filter(
                (k) =>
                  k.startsWith('sr-app-') || (k.startsWith('sr-static-') && k !== STATIC_CACHE),
              )
              .map((k) => caches.delete(k)),
          ),
        )
        .then(() => self.clients.claim()),
    );
  });

  self.addEventListener('message', (event) => {
    const type = event.data && event.data.type;
    if (type === 'SKIP_WAITING') self.skipWaiting();
    if (type === 'PURGE_APP') {
      const done = purgeApp().then(() => {
        const reply = { type: 'PURGED' };
        const port = event.ports && event.ports[0];
        if (port) port.postMessage(reply);
        else if (event.source && event.source.postMessage) event.source.postMessage(reply);
      });
      if (typeof event.waitUntil === 'function') event.waitUntil(done);
    }
  });

  self.addEventListener('fetch', (event) => {
    const kind = classify(event.request, self.location.origin);
    if (kind === 'bypass') return;
    if (kind === 'share') {
      event.respondWith(
        event.request
          .formData()
          .then(stashShare)
          .catch(() => new URLSearchParams())
          .then((q) => Response.redirect('/share' + (q.toString() ? '?' + q.toString() : ''), 303)),
      );
      return;
    }
    if (kind === 'navigate-public') {
      const surface = surfaceFor(new URL(event.request.url).pathname);
      event.respondWith(
        navigateWith(event, STATIC_CACHE, surface && surface.shell ? '/' + surface.shell : null),
      );
      return;
    }
    if (kind === 'navigate-app') {
      event.respondWith(navigateWith(event, APP_CACHE, APP_SHELL));
      return;
    }
    const cacheName = kind === 'app-file' ? APP_CACHE : STATIC_CACHE;
    event.respondWith(
      caches.open(cacheName).then((c) =>
        c.match(event.request).then(
          (hit) =>
            hit ||
            fetch(event.request).then((res) => {
              if (kind === 'app-file' && sessionLost(res)) {
                event.waitUntil(purgeApp());
              } else if (res.ok && res.type === 'basic' && !res.redirected) {
                c.put(event.request, res.clone());
              }
              return res;
            }),
        ),
      ),
    );
  });

  // Web Push (XC-PWA-004). Payload: { title, body, url?, tag? }. Shown only when no window is focused.
  self.addEventListener('push', (event) => {
    let data = {};
    try {
      data = event.data ? event.data.json() : {};
    } catch {
      /* not JSON */
    }
    event.waitUntil(
      self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
        if (wins.some((w) => w.focused)) return undefined;
        const url = safeUrl(data.url);
        return self.registration.showNotification(data.title || 'SIDE ROOM', {
          body: data.body || '',
          tag: data.tag,
          icon: '/icons/icon-192.png',
          data: { url },
        });
      }),
    );
  });

  self.addEventListener('notificationclick', (event) => {
    event.notification.close();
    const url = safeUrl(event.notification.data && event.notification.data.url);
    event.waitUntil(
      self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
        const w = wins[0];
        if (w) return w.focus().then(() => w.navigate && w.navigate(url));
        return self.clients.openWindow(url);
      }),
    );
  });
}

/** A same-origin path from a push payload; anything else opens the neutral door. */
function safeUrl(u) {
  return typeof u === 'string' && u.startsWith('/') && !u.startsWith('//') ? u : '/app';
}

if (typeof module !== 'undefined') module.exports = { classify, surfaceFor, pathMatches, safeUrl };
