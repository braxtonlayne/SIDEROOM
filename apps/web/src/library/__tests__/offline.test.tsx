// Offline reading cache (LIB-RDR-015) and border cropping (LIB-ING-007 in the reader).
import { render, screen } from '@testing-library/react';
import {
  cacheChapter,
  cachedImageUrl,
  hasRoom,
  manifestKey,
  MAX_CHAPTERS,
  OFFLINE_CACHE,
  pathKey,
  planEviction,
  readCachedManifest,
  readRegistry,
  REGISTRY_KEY,
} from '../reader/offlineCache';
import { cropView } from '../reader/model';
import { PageImage } from '../reader/PageImage';
import { manifest, pages } from './harness';

/** A minimal CacheStorage backed by maps. */
function fakeCaches() {
  const stores = new Map<string, Map<string, Response>>();
  const open = async (name: string) => {
    const m = stores.get(name) ?? new Map<string, Response>();
    stores.set(name, m);
    const keyOf = (r: RequestInfo | URL) =>
      typeof r === 'string' ? r : r instanceof Request ? new URL(r.url).pathname : String(r);
    return {
      put: async (r: RequestInfo | URL, res: Response) => void m.set(keyOf(r), res.clone()),
      match: async (r: RequestInfo | URL) => m.get(keyOf(r))?.clone(),
      delete: async (r: RequestInfo | URL) => m.delete(keyOf(r)),
      keys: async () => [...m.keys()].map((k) => new Request(`http://local.invalid${k}`)),
    };
  };
  return {
    stores,
    api: { open, delete: async (n: string) => stores.delete(n) } as unknown as CacheStorage,
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe('offline cache helpers', () => {
  it('keys pages by path without the signature', () => {
    expect(pathKey('/cdn/library/1/c1/0001.webp?exp=9&sig=abc')).toBe(
      '/cdn/library/1/c1/0001.webp',
    );
    expect(pathKey('https://x.example/cdn/a.webp?sig=1')).toBe('/cdn/a.webp');
  });

  it('keeps the newest chapters and names the ones to drop', () => {
    expect(planEviction([], 'a')).toEqual({ registry: ['a'], evict: [] });
    expect(planEviction(['a'], 'b')).toEqual({ registry: ['a', 'b'], evict: [] });
    expect(planEviction(['a', 'b'], 'c')).toEqual({ registry: ['b', 'c'], evict: ['a'] });
    // Re-opening a kept chapter moves it to the end and evicts nothing.
    expect(planEviction(['a', 'b'], 'a')).toEqual({ registry: ['b', 'a'], evict: [] });
    expect(MAX_CHAPTERS).toBe(2);
  });

  it('refuses to cache when the origin is nearly full', () => {
    expect(hasRoom(undefined)).toBe(true);
    expect(hasRoom({ usage: 100, quota: 10_000_000_000 })).toBe(true);
    expect(hasRoom({ usage: 9_500_000_000, quota: 10_000_000_000 })).toBe(false);
    expect(hasRoom({ usage: 100_000_000, quota: 150_000_000 })).toBe(false);
  });
});

describe('cacheChapter', () => {
  it('stores the manifest and pages, serves them back, and evicts the oldest chapter', async () => {
    const { api, stores } = fakeCaches();
    vi.stubGlobal('caches', api);
    vi.stubGlobal(
      'fetch',
      vi.fn(async (u: string) => new Response(`bytes of ${u}`, { status: 200 })),
    );
    const m1 = manifest(1);
    const stored = await cacheChapter(m1);
    expect(stored).toBe(3);
    expect(readRegistry()).toEqual(['c1']);
    expect((await readCachedManifest('c1'))?.chapter.id).toBe('c1');
    // A page that failed to load comes back from the cache.
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: () => 'blob:cached' }));
    expect(await cachedImageUrl('/cdn/p0.webp?sig=new')).toBe('blob:cached');
    expect(await cachedImageUrl('/cdn/never.webp')).toBeNull();
    // Re-running skips pages that are already stored.
    expect(await cacheChapter(m1)).toBe(0);
    // Two more chapters push the first one out, with its pages.
    await cacheChapter(manifest(2));
    await cacheChapter(manifest(3));
    expect(readRegistry()).toEqual(['c2', 'c3']);
    expect(await readCachedManifest('c1')).toBeNull();
    const keys = [...(stores.get(OFFLINE_CACHE)?.keys() ?? [])];
    expect(keys).toContain(manifestKey('c3'));
    expect(keys).not.toContain(manifestKey('c1'));
    expect(JSON.parse(localStorage.getItem(REGISTRY_KEY) ?? '[]')).toEqual(['c2', 'c3']);
  });

  it('does nothing without Cache Storage, for PDFs, or when the origin is full', async () => {
    expect(await cacheChapter(manifest(1))).toBe(0);
    const { api } = fakeCaches();
    vi.stubGlobal('caches', api);
    expect(
      await cacheChapter(manifest(1, { chapter: { ...manifest(1).chapter, format: 'pdf' } })),
    ).toBe(0);
    vi.stubGlobal('navigator', {
      ...navigator,
      storage: { estimate: async () => ({ usage: 99, quota: 100 }) },
    });
    expect(await cacheChapter(manifest(2))).toBe(0);
    expect(readRegistry()).toEqual([]);
  });
});

describe('border crop', () => {
  it('computes the kept region and refuses unusable boxes', () => {
    const page = { width: 1000, height: 1500 };
    const v = cropView(page, { left: 100, top: 50, right: 100, bottom: 150 })!;
    expect(v.ratio).toBe('800 / 1300');
    expect(v.widthPct).toBeCloseTo(125);
    expect(v.shiftX).toBeCloseTo(10);
    expect(v.shiftY).toBeCloseTo(3.333, 2);
    expect(cropView(page, null)).toBeNull();
    expect(cropView(page, { left: 0, top: 0, right: 0, bottom: 0 })).toBeNull();
    // Cutting away most of the page is treated as bad data.
    expect(cropView(page, { left: 500, top: 0, right: 200, bottom: 0 })).toBeNull();
    expect(cropView(page, { left: -1, top: 0, right: 0, bottom: 0 })).toBeNull();
  });

  it('shows the page whole unless cropping is on and the page has crop data', () => {
    const p = { ...pages(1)[0]!, crop: { left: 100, top: 0, right: 100, bottom: 0 } };
    const { container, rerender } = render(<PageImage page={p} cssWidth={500} alt="Page 1" />);
    expect(container.querySelector('.lib-page')).not.toHaveAttribute('data-cropped');
    rerender(<PageImage page={p} cssWidth={500} alt="Page 1" crop />);
    expect(container.querySelector('.lib-page')).toHaveAttribute('data-cropped', 'true');
    expect(screen.getByRole('img', { name: 'Page 1' })).toBeInTheDocument();
    rerender(<PageImage page={pages(1)[0]!} cssWidth={500} alt="Page 1" crop />);
    expect(container.querySelector('.lib-page')).not.toHaveAttribute('data-cropped');
  });
});
