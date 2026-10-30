// SEC-ENT-002: the logout purge, the public update toast and the build step that finishes the worker.
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  DEFAULT_SURFACES,
  finishServiceWorker,
  moveAppManifest,
  normalizeSurfaces,
  precacheFromHtml,
} from '../../vite/pwa';
import { PublicUpdateToast } from './PublicUpdateToast';
import { purgeAppCaches } from './purge';
import { __resetPwaState, setUpdateReady } from './state';

const PUBLIC = resolve(__dirname, '../../public');

describe('purgeAppCaches', () => {
  const original = {
    caches: Object.getOwnPropertyDescriptor(globalThis, 'caches'),
    sw: Object.getOwnPropertyDescriptor(navigator, 'serviceWorker'),
  };
  let store: Set<string>;

  beforeEach(() => {
    store = new Set(['sr-app-b1', 'sr-app-b0', 'sr-static-b1', 'sr-reader-offline']);
    Object.defineProperty(globalThis, 'caches', {
      configurable: true,
      value: {
        keys: async () => [...store],
        delete: async (k: string) => store.delete(k),
      },
    });
  });
  afterEach(() => {
    if (original.caches) Object.defineProperty(globalThis, 'caches', original.caches);
    else delete (globalThis as { caches?: unknown }).caches;
    if (original.sw) Object.defineProperty(navigator, 'serviceWorker', original.sw);
    else delete (navigator as { serviceWorker?: unknown }).serviceWorker;
  });

  it('without a worker, deletes the app caches itself and nothing else', async () => {
    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: { controller: null },
    });
    await purgeAppCaches();
    expect([...store].sort()).toEqual(['sr-reader-offline', 'sr-static-b1']);
  });

  it('asks the controlling worker for PURGE_APP and waits for its PURGED', async () => {
    const posted: unknown[] = [];
    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: {
        controller: {
          postMessage: (msg: unknown, ports: MessagePort[]) => {
            posted.push(msg);
            // The worker deletes, then answers on the port it was given.
            store.delete('sr-app-b1');
            store.delete('sr-app-b0');
            setTimeout(() => ports[0]!.postMessage({ type: 'PURGED' }), 5);
          },
        },
      },
    });
    await purgeAppCaches(2000);
    expect(posted).toEqual([{ type: 'PURGE_APP' }]);
    expect([...store].sort()).toEqual(['sr-reader-offline', 'sr-static-b1']);
  });

  it('does not hang on a silent worker: it gives up after the timeout and purges from the page', async () => {
    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: { controller: { postMessage: () => undefined } },
    });
    await purgeAppCaches(30);
    expect([...store].sort()).toEqual(['sr-reader-offline', 'sr-static-b1']);
  });

  it('never rejects when the worker refuses the message or storage is blocked', async () => {
    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: {
        controller: {
          postMessage: () => {
            throw new Error('detached');
          },
        },
      },
    });
    Object.defineProperty(globalThis, 'caches', {
      configurable: true,
      value: {
        keys: async () => {
          throw new Error('blocked');
        },
      },
    });
    await expect(purgeAppCaches(30)).resolves.toBeUndefined();
  });
});

describe('PublicUpdateToast', () => {
  beforeEach(() => __resetPwaState());

  it('shows nothing until an update waits, then offers Reload and Later and applies on Reload', async () => {
    const apply = vi.fn();
    render(<PublicUpdateToast />);
    expect(screen.queryByText('A new version is ready')).toBeNull();
    act(() => setUpdateReady(apply));
    expect(screen.getByText('A new version is ready')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Reload' }));
    expect(apply).toHaveBeenCalledOnce();
  });

  it('Later hides it, and it never offers an install', async () => {
    render(<PublicUpdateToast />);
    act(() => setUpdateReady(vi.fn()));
    expect(screen.queryByText(/install/i)).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(screen.queryByText('A new version is ready')).toBeNull();
  });
});

describe('the build step', () => {
  let dist = '';
  beforeEach(() => {
    dist = mkdtempSync(join(tmpdir(), 'sr-pwa-unit-'));
    writeFileSync(join(dist, 'sw.js'), readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8'));
    writeFileSync(
      join(dist, 'index.html'),
      '<script src="/entrance/landing-A.js"></script><link rel="stylesheet" href="/entrance/landing-A.css">',
    );
    writeFileSync(
      join(dist, 'auth.html'),
      '<script src="/entrance/auth-B.js"></script><link rel="modulepreload" href="/entrance/shared-C.js">' +
        '<script src="/app/leak.js"></script>',
    );
  });
  afterEach(() => rmSync(dist, { recursive: true, force: true }));

  it('fills the build id, the public precache list and the surface table from the shells that exist', () => {
    finishServiceWorker(dist, join(dist, 'none.json'));
    const sw = readFileSync(join(dist, 'sw.js'), 'utf8');
    expect(sw).not.toContain('__SR_BUILD__');
    expect(sw).not.toContain('/*__SR_PRECACHE__*/');
    expect(sw).not.toContain('/*__SR_SURFACES__*/');
    const precache = JSON.parse(/const PRECACHE = (\[.*\]);/.exec(sw)![1]!) as string[];
    expect(precache).toEqual(
      expect.arrayContaining([
        '/entrance/landing-A.js',
        '/entrance/landing-A.css',
        '/entrance/auth-B.js',
        '/entrance/shared-C.js',
        '/manifest.webmanifest',
        '/offline.css',
        '/icons/icon-192.png',
      ]),
    );
    // The worker never precaches a signed-in file, even if a shell wrongly referenced one.
    expect(precache.some((p) => p.startsWith('/app'))).toBe(false);
    const surfaces = JSON.parse(/const SURFACES = (\[.*\]);/.exec(sw)![1]!) as Array<{
      shell: string | null;
    }>;
    // invite.html was not built: its surface is left out; the plain legal page stays.
    expect(surfaces.map((s) => s.shell)).toEqual(['index.html', 'auth.html', null]);
  });

  it('changes the build id when a shell changes', () => {
    finishServiceWorker(dist, join(dist, 'none.json'));
    const a = /const BUILD = '([0-9a-f]+)'/.exec(readFileSync(join(dist, 'sw.js'), 'utf8'))![1];
    writeFileSync(join(dist, 'sw.js'), readFileSync(resolve(PUBLIC, 'sw.js'), 'utf8'));
    writeFileSync(join(dist, 'index.html'), '<script src="/entrance/landing-Z.js"></script>');
    finishServiceWorker(dist, join(dist, 'none.json'));
    const b = /const BUILD = '([0-9a-f]+)'/.exec(readFileSync(join(dist, 'sw.js'), 'utf8'))![1];
    expect(b).not.toBe(a);
  });

  it('moves the signed-in manifest out of the public root into the gated directory', () => {
    writeFileSync(join(dist, 'app-manifest.webmanifest'), '{"shortcuts":[]}');
    mkdirSync(join(dist, 'app'));
    moveAppManifest(dist);
    expect(existsSync(join(dist, 'app-manifest.webmanifest'))).toBe(false);
    expect(readFileSync(join(dist, 'app', 'manifest.webmanifest'), 'utf8')).toBe(
      '{"shortcuts":[]}',
    );
    moveAppManifest(dist); // idempotent when there is nothing to move
  });

  it('reads the surface table in either shape and falls back to the design table', () => {
    const list = [{ shell: 'index.html', paths: ['/'] }, { html: '/x.html', path: '/x' }, 'junk'];
    const want = [
      { shell: 'index.html', paths: ['/'] },
      { shell: 'x.html', paths: ['/x'] },
    ];
    expect(normalizeSurfaces(list)).toEqual(want);
    expect(normalizeSurfaces({ surfaces: list })).toEqual(want);
    expect(normalizeSurfaces({ nothing: true })).toBe(DEFAULT_SURFACES);
    expect(normalizeSurfaces([{ shell: 'a.html', paths: ['no-slash'] }])).toBe(DEFAULT_SURFACES);
  });

  it('takes no signed-in file from a shell', () => {
    expect([...precacheFromHtml('<script src="/app/x.js"></script>')]).toEqual([]);
  });
});
