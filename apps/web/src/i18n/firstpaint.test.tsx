import { act, render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { isCorePath } from './corePaths';

/**
 * First paint must not wait for the rest of the message catalog (en.rest chunk), and no message key may
 * ever show. `test/setup.ts` loads the whole catalog, so these tests reset the module registry to get a
 * fresh app that only has the core fragments (entrance, a11y, pwa), like the browser on first paint.
 */
const KEY_SHAPE = /\b[a-z][a-z0-9]*\.[A-Za-z0-9]+\.[A-Za-z0-9.]+\b/;

async function freshApp(path: string) {
  vi.resetModules();
  const { createAppRouter } = await import('../router');
  const store = await import('./messages/store');
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return { router, store };
}

describe('first paint with only the core catalog', () => {
  let warn: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  });
  afterEach(() => warn.mockRestore());

  it.each([
    ['/', /with the door/],
    ['/login', 'Come on in.'],
    ['/login/2fa', 'Your code.'],
    ['/forgot', /fix that/],
    ['/reset/tok', 'Choose a new one.'],
    ['/no-invitation', 'No invitation?'],
    ['/house-notes', 'House notes'],
  ])('renders the entrance route %s without the rest catalog', async (path, heading) => {
    const { store } = await freshApp(path);
    expect(store.isRestLoaded()).toBe(false);
    expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
    expect(document.body.textContent ?? '').not.toMatch(KEY_SHAPE);
    const i18nWarnings = warn.mock.calls.filter((c: unknown[]) => String(c[0]).startsWith('i18n:'));
    expect(i18nWarnings).toEqual([]);
  });

  it('holds a non-entrance route until the full catalog arrives, never showing keys', async () => {
    const { store } = await freshApp('/channels');
    expect(store.isRestLoaded()).toBe(false);
    expect(await screen.findByRole('status', { busy: true })).toBeInTheDocument();
    expect(document.body.textContent ?? '').not.toMatch(KEY_SHAPE);
    expect(screen.queryByRole('heading', { name: 'Friends' })).toBeNull();

    await act(async () => {
      await import('./messages/en.rest');
    });
    expect(await screen.findByRole('heading', { name: 'Friends' })).toBeInTheDocument();
    expect(document.body.textContent ?? '').not.toMatch(KEY_SHAPE);
  });

  it('useT subscribers re-render when the catalog grows', async () => {
    vi.resetModules();
    const { useT } = await import('./react');
    const store = await import('./messages/store');
    function Probe() {
      const t = useT();
      return <p data-testid="p">{t('admin.ui.cancel')}</p>;
    }
    render(<Probe />);
    expect(store.isRestLoaded()).toBe(false);
    expect(screen.getByTestId('p')).toHaveTextContent('admin.ui.cancel'); // the key fallback, which gates keep off screen
    await act(async () => {
      await import('./messages/en.rest');
    });
    expect(screen.getByTestId('p')).toHaveTextContent('Cancel');
  });
});

describe('core paths', () => {
  it('lists the entrance and nothing app-like', () => {
    for (const p of ['/', '/login', '/register', '/join/abc', '/reset/x']) {
      expect(isCorePath(p), p).toBe(true);
    }
    for (const p of [
      '/channels',
      '/app',
      '/settings',
      '/admin',
      '/support',
      '/support/leaderboards',
      '/watch/1',
    ]) {
      expect(isCorePath(p), p).toBe(false);
    }
  });
});

// ---- Static check: every key the entrance routes use is in the core fragments ----
const SRC = resolve(__dirname, '..');
const CORE_FRAGMENTS = ['entrance', 'a11y', 'pwa'];

function coreKeys(): Set<string> {
  const keys = new Set<string>();
  for (const f of CORE_FRAGMENTS) {
    const src = readFileSync(join(SRC, 'i18n', 'messages', 'en', `${f}.ts`), 'utf8');
    for (const m of src.matchAll(/^\s*'([\w.]+)':/gm)) keys.add(m[1]!);
  }
  return keys;
}

function resolveImport(from: string, spec: string): string | undefined {
  const base = resolve(dirname(from), spec);
  for (const c of [`${base}.ts`, `${base}.tsx`, join(base, 'index.ts'), join(base, 'index.tsx')]) {
    if (existsSync(c)) return c;
  }
  return undefined;
}

/** Files statically imported (transitively) from `entry`; dynamic `import()` is a separate chunk, skipped. */
function staticClosure(entry: string, seen = new Set<string>()): Set<string> {
  if (seen.has(entry)) return seen;
  seen.add(entry);
  const src = readFileSync(entry, 'utf8');
  for (const m of src.matchAll(/^\s*(?:import|export)\s[^'"]*?from\s+['"](\.[^'"]+)['"]/gms)) {
    const next = resolveImport(entry, m[1]!);
    if (next) staticClosure(next, seen);
  }
  return seen;
}

describe('entrance routes only use core-fragment keys', () => {
  it('every literal t("...") key in the entrance components is in entrance/a11y/pwa', () => {
    const dir = join(SRC, 'routes', 'entrance');
    const entries = readdirSync(dir)
      .filter((f) => f.endsWith('.tsx') && !f.endsWith('.test.tsx'))
      .map((f) => join(dir, f));
    expect(entries.length).toBeGreaterThan(5);
    const core = coreKeys();
    const missing: string[] = [];
    // Entrance-only components: shared shells and forms. Files outside this folder that the entrance
    // imports are scanned too, so a shared component cannot smuggle in a non-core key.
    for (const entry of entries) {
      for (const file of staticClosure(entry)) {
        if (/\.test\.tsx?$/.test(file)) continue;
        for (const m of readFileSync(file, 'utf8').matchAll(
          /\bt\(\s*(['"`])([a-z0-9][\w.]*\.[\w.]+)\1/g,
        )) {
          if (!core.has(m[2]!)) missing.push(`${file.slice(SRC.length + 1)}: ${m[2]}`);
        }
      }
    }
    expect([...new Set(missing)].sort()).toEqual([]);
  });
});
