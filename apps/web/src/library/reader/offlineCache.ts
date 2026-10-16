// Offline reading cache (LIB-RDR-015): the reader keeps the current and the next chapter in the browser's
// Cache Storage so they open without a connection. This is a cache, not a download (downloads are the
// Patron file export, D-075). It lives in the page and not in the service worker on purpose: the worker never
// touches /cdn (signed, access-controlled media, see public/sw.js), so nothing authenticated is stored by it.
//
// Entries are keyed by the URL path without its signature, so a re-signed URL still hits. At most
// MAX_CHAPTERS chapters are kept (the oldest goes first), nothing is stored when the origin is nearly out of
// quota, and a quota error while writing stops the caching and drops the oldest chapter.
import type { PageManifest } from '../types';
import { pickVariant } from './model';

export const OFFLINE_CACHE = 'sr-library-offline-v1';
export const REGISTRY_KEY = 'sr.library.offline.v1';
/** Chapters kept: the one being read and the next. */
export const MAX_CHAPTERS = 2;
/** Do not cache when the origin has used more than this share of its quota. */
export const MAX_QUOTA_SHARE = 0.85;
/** ... or has less free space than this. */
export const MIN_FREE_BYTES = 80 * 1024 * 1024;
const CONCURRENCY = 3;

/** `/cdn/library/1/2/0001.webp?exp=..&sig=..` -> `/cdn/library/1/2/0001.webp`. */
export function pathKey(url: string): string {
  const noQuery = url.split('?')[0] ?? url;
  try {
    return new URL(noQuery, 'http://local.invalid').pathname;
  } catch {
    return noQuery;
  }
}

export const manifestKey = (chapterId: string) =>
  `/_offline/library/manifest/${encodeURIComponent(chapterId)}`;

/** The URL cached for a page: the 1080 px variant (or the best available). */
export const cacheUrlFor = (p: PageManifest['pages'][number]) => pickVariant(p, 1080, 1);

export function readRegistry(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(REGISTRY_KEY) ?? '[]') as unknown;
    return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}
function writeRegistry(ids: string[]) {
  try {
    localStorage.setItem(REGISTRY_KEY, JSON.stringify(ids));
  } catch {
    /* best effort */
  }
}

/** The new registry after adding `id` (moved to the end if present) and the chapters to evict. */
export function planEviction(
  registry: string[],
  id: string,
  keep = MAX_CHAPTERS,
): { registry: string[]; evict: string[] } {
  const next = [...registry.filter((x) => x !== id), id];
  const evict = next.slice(0, Math.max(0, next.length - keep));
  return { registry: next.slice(evict.length), evict };
}

/** Whether the origin has room for more (unknown estimates count as room). */
export function hasRoom(estimate: { usage?: number; quota?: number } | undefined): boolean {
  if (!estimate?.quota) return true;
  const used = estimate.usage ?? 0;
  return used / estimate.quota <= MAX_QUOTA_SHARE && estimate.quota - used >= MIN_FREE_BYTES;
}

const supported = () => typeof caches !== 'undefined';

async function evict(chapterId: string) {
  if (!supported()) return;
  const cache = await caches.open(OFFLINE_CACHE);
  await cache.delete(manifestKey(chapterId));
  for (const req of await cache.keys()) {
    if (pathKey(req.url).includes(`/${chapterId}/`)) await cache.delete(req);
  }
}

/** Remember `id` as the newest chapter and evict the ones beyond the limit. */
async function touch(id: string) {
  const plan = planEviction(readRegistry(), id);
  writeRegistry(plan.registry);
  for (const old of plan.evict) await evict(old);
}

/**
 * Store a chapter's manifest and page images. Resolves with how many pages were stored; stops quietly
 * when the quota runs out. Safe to call again for the same chapter (already stored pages are skipped).
 */
export async function cacheChapter(manifest: PageManifest, signal?: AbortSignal): Promise<number> {
  if (!supported() || manifest.chapter.format !== 'images') return 0;
  const est = await navigator.storage?.estimate?.().catch(() => undefined);
  if (!hasRoom(est)) return 0;
  const id = manifest.chapter.id;
  await touch(id);
  const cache = await caches.open(OFFLINE_CACHE);
  let stored = 0;
  try {
    await cache.put(
      manifestKey(id),
      new Response(JSON.stringify(manifest), { headers: { 'content-type': 'application/json' } }),
    );
    const queue = manifest.pages.map(cacheUrlFor);
    const worker = async () => {
      for (let url = queue.shift(); url !== undefined; url = queue.shift()) {
        if (signal?.aborted) return;
        const key = pathKey(url);
        if (await cache.match(key)) continue;
        const res = await fetch(url, { signal }).catch(() => undefined);
        if (!res?.ok) continue;
        await cache.put(key, res);
        stored += 1;
      }
    };
    await Promise.all(Array.from({ length: CONCURRENCY }, worker));
  } catch (e) {
    // A full origin throws QuotaExceededError: drop the oldest chapter and give up for now.
    const [oldest] = readRegistry();
    if (oldest && oldest !== id) {
      writeRegistry(readRegistry().filter((x) => x !== oldest));
      await evict(oldest);
    }
    if (!(e instanceof DOMException) && (e as Error)?.name !== 'AbortError') throw e;
  }
  return stored;
}

/** The manifest stored for a chapter, for opening it offline. */
export async function readCachedManifest(chapterId: string): Promise<PageManifest | null> {
  if (!supported()) return null;
  try {
    const hit = await (await caches.open(OFFLINE_CACHE)).match(manifestKey(chapterId));
    return hit ? ((await hit.json()) as PageManifest) : null;
  } catch {
    return null;
  }
}

/** An object URL for a page image that failed to load, when it is in the cache; revoke it when done. */
export async function cachedImageUrl(url: string): Promise<string | null> {
  if (!supported()) return null;
  try {
    const cache = await caches.open(OFFLINE_CACHE);
    // The page may have been cached at another size than the one asked for: try the exact path first.
    const hit = await cache.match(pathKey(url));
    return hit ? URL.createObjectURL(await hit.blob()) : null;
  } catch {
    return null;
  }
}

/** Forget everything (the settings panel offers it). */
export async function clearOfflineCache() {
  writeRegistry([]);
  if (supported()) await caches.delete(OFFLINE_CACHE);
}
