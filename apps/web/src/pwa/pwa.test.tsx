import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { precacheFromHtml } from '../../vite/pwa';
import { PwaHost } from './PwaHost';
import {
  __resetPwaState,
  applyPendingUpdate,
  flushDeferredUpdate,
  isIosSafari,
  setNoticesBarMounted,
  setUpdateGuard,
  setUpdateReady,
} from './state';

const PUBLIC = resolve(__dirname, '../../public');

// sw.js is a classic worker script; run it with no `self` so only its exported policy is evaluated.
const SURFACES = [
  { shell: 'index.html', paths: ['/', '/join/$code'] },
  { shell: 'auth.html', paths: ['/login', '/reset/$token'] },
  { shell: 'profile.html', paths: ['/~$handle'] },
  { shell: null, paths: ['/house-notes'] },
];
const mod = {
  exports: {} as {
    classify: (r: Request, origin: string) => string;
    pathMatches: (pattern: string, path: string) => boolean;
    safeUrl: (u: unknown) => string;
  },
};
new Function(
  'module',
  'self',
  readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8').replace(
    '/*__SR_SURFACES__*/ []',
    JSON.stringify(SURFACES),
  ),
)(mod, undefined);
const sw = mod.exports;

const ORIGIN = 'https://side.example';
// A plain object: the Request constructor refuses mode "navigate", which only the browser can set.
const req = (
  path: string,
  init: { method?: string; mode?: string; headers?: Record<string, string>; origin?: string } = {},
) => {
  const lower = Object.fromEntries(
    Object.entries(init.headers ?? {}).map(([k, v]) => [k.toLowerCase(), v]),
  );
  return {
    url: (init.origin ?? ORIGIN) + path,
    method: init.method ?? 'GET',
    mode: init.mode ?? 'no-cors',
    headers: {
      get: (k: string) => lower[k.toLowerCase()] ?? null,
      has: (k: string) => k.toLowerCase() in lower,
    },
  } as unknown as Request;
};

describe('service worker policy', () => {
  it('bypasses every non-page request that is not a public file, without naming any prefix', () => {
    for (const p of [
      '/api/v1/users/@me',
      '/api/v10/sr/auth/login',
      '/gateway',
      '/gateway?v=10',
      '/cdn/attachments/1/2/a.png',
      '/internal/metrics',
      '/developers/docs/api',
      '/oauth/discord/start',
      '/THIRD_PARTY_NOTICES.md',
    ]) {
      expect(sw.classify(req(p), ORIGIN), p).toBe('bypass');
    }
    // A document navigation to a file is the browser's business too.
    expect(sw.classify(req('/THIRD_PARTY_NOTICES.md', { mode: 'navigate' }), ORIGIN)).toBe(
      'bypass',
    );
    expect(sw.classify(req('/cdn/a/b.png', { mode: 'navigate' }), ORIGIN)).toBe('bypass');
  });

  it('matches a public path against its surface pattern', () => {
    expect(sw.pathMatches('/join/$code', '/join/abc')).toBe(true);
    expect(sw.pathMatches('/join/$code', '/join/')).toBe(false);
    expect(sw.pathMatches('/join/$code', '/join/abc/def')).toBe(false);
    expect(sw.pathMatches('/~$handle', '/~ada')).toBe(true);
    expect(sw.pathMatches('/~$handle', '/~')).toBe(false);
    expect(sw.pathMatches('/', '/x')).toBe(false);
    expect(sw.pathMatches('/house-notes#notes-rules', '/house-notes')).toBe(true);
    expect(sw.pathMatches('/login', '/login/')).toBe(true);
  });

  it('takes a public page and any other page as two different navigations', () => {
    for (const p of ['/', '/join/abc', '/login', '/reset/t0k', '/~ada', '/house-notes']) {
      expect(sw.classify(req(p, { mode: 'navigate' }), ORIGIN), p).toBe('navigate-public');
    }
    for (const p of ['/app', '/channels/1/2', '/anything-else', '/login/2fa']) {
      expect(sw.classify(req(p, { mode: 'navigate' }), ORIGIN), p).toBe('navigate-app');
    }
    // The plain offline page and the error pages are files: never a shell, never answered with one.
    for (const p of ['/offline.html', '/404.html', '/50x.html']) {
      expect(sw.classify(req(p, { mode: 'navigate' }), ORIGIN), p).toBe('bypass');
    }
  });

  it('puts files under the signed-in prefix in their own class', () => {
    expect(sw.classify(req('/app/chunk-AbC.js'), ORIGIN)).toBe('app-file');
    expect(sw.classify(req('/app/chunk-AbC.js', { headers: { Range: 'bytes=0-1' } }), ORIGIN)).toBe(
      'bypass',
    );
    expect(sw.classify(req('/app/x.js', { method: 'POST' }), ORIGIN)).toBe('bypass');
    expect(sw.classify(req('/entrance/landing-AbC.js'), ORIGIN)).toBe('static');
  });

  it('opens a push or a click on a same-origin path, else on the neutral door', () => {
    expect(sw.safeUrl('/x/y?z=1')).toBe('/x/y?z=1');
    expect(sw.safeUrl('//evil.example/x')).toBe('/app');
    expect(sw.safeUrl('https://evil.example')).toBe('/app');
    expect(sw.safeUrl(undefined)).toBe('/app');
  });

  it('intercepts only the Share Target POST to /share', () => {
    expect(sw.classify(req('/share', { method: 'POST' }), ORIGIN)).toBe('share');
    expect(
      sw.classify(req('/share', { method: 'POST', origin: 'https://o.example' }), ORIGIN),
    ).toBe('bypass');
    expect(sw.classify(req('/share/x', { method: 'POST' }), ORIGIN)).toBe('bypass');
    expect(sw.classify(req('/api/v10/share', { method: 'POST' }), ORIGIN)).toBe('bypass');
  });

  it('declares a POST multipart share target with files', () => {
    const m = JSON.parse(readFileSync(resolve(PUBLIC, 'manifest.webmanifest'), 'utf8'));
    expect(m.share_target.method).toBe('POST');
    expect(m.share_target.enctype).toBe('multipart/form-data');
    expect(m.share_target.params.files[0].name).toBe('files');
  });

  it('never intercepts mutations, cross-origin, authorised or ranged requests', () => {
    expect(sw.classify(req('/assets/a.js', { method: 'POST' }), ORIGIN)).toBe('bypass');
    expect(sw.classify(req('/assets/a.js', { origin: 'https://other.example' }), ORIGIN)).toBe(
      'bypass',
    );
    expect(
      sw.classify(req('/assets/a.js', { headers: { Authorization: 'Bearer x' } }), ORIGIN),
    ).toBe('bypass');
    expect(sw.classify(req('/assets/a.mp4', { headers: { Range: 'bytes=0-1' } }), ORIGIN)).toBe(
      'bypass',
    );
  });

  it('caches public build output', () => {
    for (const p of [
      '/assets/index-abc.js',
      '/fonts/inter.woff2',
      '/icons/icon-192.png',
      '/manifest.webmanifest',
    ]) {
      expect(sw.classify(req(p), ORIGIN), p).toBe('static');
    }
    // Unknown subresources (data fetches from a future route) are left to the network.
    expect(sw.classify(req('/something.json'), ORIGIN)).toBe('bypass');
  });

  it('keeps its placeholders for the build step and installs no unsafe handlers', () => {
    const src = readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8');
    expect(src).toContain('__SR_BUILD__');
    expect(src).toContain('/*__SR_PRECACHE__*/ []');
    expect(src).toContain('/*__SR_SURFACES__*/ []');
    expect(src).not.toMatch(/importScripts|eval\(/);
  });
});

describe('build plugin helpers', () => {
  it('extracts the entry assets from the built index.html', () => {
    const html =
      '<script type="module" crossorigin src="/assets/index-AbC.js"></script>' +
      '<link rel="modulepreload" href="/assets/vendor-X.js">' +
      '<link rel="stylesheet" href="/assets/index-Q.css">' +
      '<link rel="preload" as="font" href="/fonts/newsreader-variable.woff2">' +
      '<link rel="manifest" href="/manifest.webmanifest">' +
      '<link rel="icon" href="https://cdn.example/x.js">';
    expect([...precacheFromHtml(html)].sort()).toEqual([
      '/assets/index-AbC.js',
      '/assets/index-Q.css',
      '/assets/vendor-X.js',
      '/fonts/newsreader-variable.woff2',
      '/manifest.webmanifest',
    ]);
  });

  it('ships a public manifest with the neutral door, no shortcuts and a maskable icon', () => {
    const m = JSON.parse(readFileSync(resolve(PUBLIC, 'manifest.webmanifest'), 'utf8'));
    expect(m.display).toBe('standalone');
    expect(m.start_url).toBe('/app');
    expect(m.id).toBe('/');
    expect(m.shortcuts).toBeUndefined();
    expect(m.icons.some((i: { purpose: string }) => i.purpose === 'maskable')).toBe(true);
    for (const i of m.icons)
      expect(readFileSync(resolve(PUBLIC, i.src.slice(1))).length).toBeGreaterThan(100);
  });

  it('ships the signed-in manifest: the same app (id, scope, icons) plus the shortcuts', () => {
    const pub = JSON.parse(readFileSync(resolve(PUBLIC, 'manifest.webmanifest'), 'utf8'));
    const app = JSON.parse(readFileSync(resolve(PUBLIC, 'app-manifest.webmanifest'), 'utf8'));
    for (const k of ['id', 'scope', 'start_url', 'name', 'icons', 'share_target'])
      expect(app[k], k).toEqual(pub[k]);
    expect(app.shortcuts.map((s: { name: string }) => s.name)).toEqual([
      'Lounge',
      'Cinema',
      'Library',
    ]);
  });

  it('names no section in the worker, the public manifest or the plain pages', () => {
    const words =
      /Lounge|Cinema|Library|Soundboard|Activities|Staff|jukebox|\/channels|\/cinema|\/library|\/gateway|\/cdn|\/voice|\/developers|\/help|\/api|\/internal/i;
    for (const f of [
      'sw.js',
      'manifest.webmanifest',
      'offline.html',
      'offline.css',
      '404.html',
      '50x.html',
    ])
      expect(readFileSync(resolve(PUBLIC, f), 'utf8'), f).not.toMatch(words);
    for (const f of ['offline.html', '404.html', '50x.html']) {
      const html = readFileSync(resolve(PUBLIC, f), 'utf8');
      expect(html, f).not.toMatch(/<script|\son[a-z]+=/i);
    }
  });
});

describe('PwaHost', () => {
  beforeEach(() => {
    __resetPwaState();
    localStorage.clear();
    window.history.pushState({}, '', '/channels');
  });

  it('offers a reload when an update is waiting, and applies it on click', async () => {
    const apply = vi.fn();
    render(<PwaHost now={0} />);
    expect(screen.queryByText('A new version is ready')).toBeNull();
    act(() => setUpdateReady(apply));
    expect(screen.getByText('A new version is ready')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Reload' }));
    expect(apply).toHaveBeenCalledOnce();
  });

  it('G-49 shows no toast while the Lounge notices bar is mounted', () => {
    render(<PwaHost now={0} />);
    act(() => setNoticesBarMounted(true));
    act(() => setUpdateReady(vi.fn()));
    expect(screen.queryByText('A new version is ready')).toBeNull();
    act(() => setNoticesBarMounted(false));
    expect(screen.getByText('A new version is ready')).toBeInTheDocument();
  });

  it('holds back the install nudge until the browser offers one', async () => {
    render(<PwaHost now={0} />);
    await act(() => new Promise((r) => setTimeout(r, 10)));
    expect(screen.queryByText('Install SIDE ROOM')).toBeNull();
    const ev = Object.assign(new Event('beforeinstallprompt', { cancelable: true }), {
      prompt: vi.fn().mockResolvedValue(undefined),
      userChoice: Promise.resolve({ outcome: 'accepted' as const }),
    });
    act(() => void window.dispatchEvent(ev));
    expect(ev.defaultPrevented).toBe(true);
    expect(screen.getByText('Install SIDE ROOM')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    expect(ev.prompt).toHaveBeenCalledOnce();
  });

  it('detects iOS Safari for Add to Home Screen instructions', () => {
    const safari =
      'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1';
    expect(isIosSafari(safari, 5)).toBe(true);
    expect(isIosSafari(safari.replace('Version/17.0', 'CriOS/120'), 5)).toBe(false);
    expect(isIosSafari('Mozilla/5.0 (Windows NT 10.0) Chrome/120', 0)).toBe(false);
  });
});

describe('update while the floating player plays (MUS-PLY-001)', () => {
  beforeEach(() => __resetPwaState());

  it('waits for the track boundary instead of reloading, and says so', async () => {
    const apply = vi.fn();
    let playing = true;
    setUpdateGuard(() => playing);
    render(<PwaHost now={0} />);
    act(() => setUpdateReady(apply));
    await userEvent.click(screen.getByRole('button', { name: 'Reload' }));
    expect(apply).not.toHaveBeenCalled();
    expect(screen.getByText('The update will install when this track ends.')).toBeInTheDocument();
    // A flush while still playing changes nothing.
    expect(flushDeferredUpdate()).toBe(false);
    playing = false;
    act(() => void flushDeferredUpdate());
    expect(apply).toHaveBeenCalledOnce();
    expect(flushDeferredUpdate()).toBe(false);
  });

  it('reloads at once when no guard holds it back', async () => {
    const apply = vi.fn();
    const off = setUpdateGuard(() => false);
    act(() => setUpdateReady(apply));
    act(() => applyPendingUpdate());
    expect(apply).toHaveBeenCalledOnce();
    off();
  });
});
