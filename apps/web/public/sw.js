/*
 * SIDE ROOM service worker (XC-PWA-002).
 *
 * A plain classic script: copied verbatim from public/, then `pwaPlugin()` in vite.config.ts replaces the
 * two placeholders below with the build id and the entry-asset list. It is registered in production builds
 * only (src/pwa/register.ts).
 *
 * What it caches, and what it never touches:
 *   - Precache: the app shell (index.html), the entry JS/CSS from the build, preloaded fonts, icons, the
 *     offline page. Other hashed files under /assets are cached the first time they are used (cache-first,
 *     safe because the file name contains the content hash).
 *   - Navigations: network first; the last good shell is the offline fallback, then /offline.html.
 *   - NEVER handled (the browser talks to the network directly): non-GET requests, other origins, /api,
 *     /gateway, /cdn (signed, access-controlled media), /internal, /developers, /oauth, requests carrying an
 *     Authorization header, and Range requests. Nothing authenticated or user-specific is ever stored here.
 *   - Updates: a new worker waits; the page shows "A new version is ready" and posts SKIP_WAITING.
 */
const BUILD = '__SR_BUILD__';
const PRECACHE = /*__SR_PRECACHE__*/ [];
const CACHE = 'sr-static-' + BUILD;
const SHELL = '/index.html';
const OFFLINE = '/offline.html';

const BYPASS_PREFIXES = ['/api/', '/gateway', '/cdn/', '/internal/', '/developers/', '/oauth/'];
const STATIC_PREFIXES = ['/assets/', '/fonts/', '/icons/', '/entrance/'];
const STATIC_FILES = ['/manifest.webmanifest', '/offline.css', '/offline.html'];

/** 'bypass' (do not intercept), 'navigate' (app shell), or 'static' (immutable build output). */
function classify(request, origin) {
  if (request.method !== 'GET') return 'bypass';
  const url = new URL(request.url);
  if (url.origin !== origin) return 'bypass';
  const p = url.pathname;
  if (BYPASS_PREFIXES.some((x) => p === x.replace(/\/$/, '') || p.startsWith(x))) return 'bypass';
  if (request.headers.get('authorization') || request.headers.has('range')) return 'bypass';
  if (request.mode === 'navigate') return 'navigate';
  if (STATIC_PREFIXES.some((x) => p.startsWith(x)) || STATIC_FILES.includes(p)) return 'static';
  return 'bypass';
}

if (typeof self !== 'undefined' && typeof self.addEventListener === 'function') {
  self.addEventListener('install', (event) => {
    event.waitUntil(
      caches.open(CACHE).then((cache) =>
        // `reload` bypasses the HTTP cache so the shell and entry assets match this build.
        Promise.all(
          [SHELL, OFFLINE, ...PRECACHE].map((u) =>
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
              .filter((k) => k.startsWith('sr-static-') && k !== CACHE)
              .map((k) => caches.delete(k)),
          ),
        )
        .then(() => self.clients.claim()),
    );
  });

  self.addEventListener('message', (event) => {
    if (event.data && event.data.type === 'SKIP_WAITING') self.skipWaiting();
  });

  self.addEventListener('fetch', (event) => {
    const kind = classify(event.request, self.location.origin);
    if (kind === 'bypass') return;
    if (kind === 'navigate') {
      event.respondWith(
        fetch(event.request)
          .then((res) => {
            const ct = res.headers.get('content-type') || '';
            if (res.ok && !res.redirected && ct.includes('text/html')) {
              const copy = res.clone();
              caches.open(CACHE).then((c) => c.put(SHELL, copy));
            }
            return res;
          })
          .catch(() =>
            caches.open(CACHE).then((c) => c.match(SHELL).then((hit) => hit || c.match(OFFLINE))),
          ),
      );
      return;
    }
    event.respondWith(
      caches.open(CACHE).then((c) =>
        c.match(event.request).then(
          (hit) =>
            hit ||
            fetch(event.request).then((res) => {
              if (res.ok && res.type === 'basic') c.put(event.request, res.clone());
              return res;
            }),
        ),
      ),
    );
  });

  // Web Push (XC-PWA-004). Payload: { title, body, url?, tag? }. Shown only when no app window is focused.
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
        const url =
          typeof data.url === 'string' && data.url.startsWith('/') ? data.url : '/channels';
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
    const url = (event.notification.data && event.notification.data.url) || '/channels';
    event.waitUntil(
      self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
        const w = wins[0];
        if (w) return w.focus().then(() => w.navigate && w.navigate(url));
        return self.clients.openWindow(url);
      }),
    );
  });
}

if (typeof module !== 'undefined') module.exports = { classify, BYPASS_PREFIXES };
