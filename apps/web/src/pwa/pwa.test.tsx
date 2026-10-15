import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { precacheFromHtml } from '../../vite/pwa';
import { PwaHost } from './PwaHost';
import { __resetPwaState, isIosSafari, setUpdateReady } from './state';

const PUBLIC = resolve(__dirname, '../../public');

// sw.js is a classic worker script; run it with no `self` so only its exported policy is evaluated.
const mod = { exports: {} as { classify: (r: Request, origin: string) => string } };
new Function('module', 'self', readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8'))(mod, undefined);
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
  it('never intercepts API, gateway, media, auth or internal traffic', () => {
    for (const p of [
      '/api/v1/users/@me',
      '/api/v10/sr/auth/login',
      '/gateway',
      '/gateway?v=10',
      '/cdn/attachments/1/2/a.png',
      '/internal/metrics',
      '/developers/docs/api',
      '/oauth/discord/start',
    ]) {
      expect(sw.classify(req(p), ORIGIN), p).toBe('bypass');
      expect(sw.classify(req(p, { mode: 'navigate' }), ORIGIN), `nav ${p}`).toBe('bypass');
    }
  });

  it('intercepts only the Share Target POST to /share', () => {
    expect(sw.classify(req('/share', { method: 'POST' }), ORIGIN)).toBe('share');
    expect(sw.classify(req('/share', { method: 'POST', origin: 'https://o.example' }), ORIGIN)).toBe(
      'bypass',
    );
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

  it('caches build output and treats page loads as the app shell', () => {
    for (const p of [
      '/assets/index-abc.js',
      '/fonts/inter.woff2',
      '/icons/icon-192.png',
      '/manifest.webmanifest',
    ]) {
      expect(sw.classify(req(p), ORIGIN), p).toBe('static');
    }
    expect(sw.classify(req('/channels/1/2', { mode: 'navigate' }), ORIGIN)).toBe('navigate');
    // Unknown subresources (data fetches from a future route) are left to the network.
    expect(sw.classify(req('/something.json'), ORIGIN)).toBe('bypass');
  });

  it('keeps its placeholders for the build step and installs no unsafe handlers', () => {
    const src = readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8');
    expect(src).toContain('__SR_BUILD__');
    expect(src).toContain('/*__SR_PRECACHE__*/ []');
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

  it('ships a valid manifest with the required shortcuts and a maskable icon', () => {
    const m = JSON.parse(readFileSync(resolve(PUBLIC, 'manifest.webmanifest'), 'utf8'));
    expect(m.display).toBe('standalone');
    expect(m.icons.some((i: { purpose: string }) => i.purpose === 'maskable')).toBe(true);
    expect(m.shortcuts.map((s: { name: string }) => s.name)).toEqual([
      'Lounge',
      'Cinema',
      'Library',
    ]);
    for (const i of m.icons)
      expect(readFileSync(resolve(PUBLIC, i.src.slice(1))).length).toBeGreaterThan(100);
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
