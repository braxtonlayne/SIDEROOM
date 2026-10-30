/**
 * MUS-LIB-001: the smart queue builders. They fetch candidates from the member's reachable library (the
 * server filters by album, artist and mount; genre and likeness are decided here, over what the server
 * returned) and hand the list to the pure rules in `queue.ts`.
 *
 * The scan for genre and "more like this" reads at most `SCAN_LIMIT` tracks. A bigger library gets a
 * representative start, not the whole set; the client-side search of wave 32 replaces the scan.
 */
import { byAlbum, byArtist, byGenre, dedupe, moreLikeThis, seededRng, shuffled, type Rng } from './queue';
import type { LibraryTrack } from './types';

export const PAGE = 200;
export const SCAN_LIMIT = 1000;

export interface LibraryQuery {
  q?: string;
  mount?: string;
  album?: string;
  artist?: string;
  limit?: number;
  offset?: number;
}

export interface LibraryPage {
  items: LibraryTrack[];
  total: number;
}

export type Fetcher = (query: LibraryQuery) => Promise<LibraryPage>;

export type SmartKind = 'album' | 'artist' | 'genre' | 'similar';

export interface SmartResult {
  tracks: LibraryTrack[];
  startAt: number;
}

/** Read pages until `total` or `max` tracks. */
export async function scan(
  fetch: Fetcher,
  base: LibraryQuery = {},
  max = SCAN_LIMIT,
): Promise<LibraryTrack[]> {
  const out: LibraryTrack[] = [];
  for (let offset = 0; offset < max; offset += PAGE) {
    const page = await fetch({ ...base, limit: PAGE, offset });
    out.push(...page.items);
    if (page.items.length < PAGE || out.length >= page.total) break;
  }
  return out.slice(0, max);
}

/**
 * Build the queue for `kind` around `seed`. With `shuffle` the list is a random permutation (each track once)
 * that opens with the seed; without it the list is in its natural order and `startAt` points at the seed.
 */
export async function buildSmartQueue(
  kind: SmartKind,
  seed: LibraryTrack,
  fetch: Fetcher,
  opts: { shuffle?: boolean; rng?: Rng } = {},
): Promise<SmartResult> {
  const who = seed.albumArtist || seed.artist || '';
  let picked: SmartResult;
  switch (kind) {
    case 'album': {
      if (!seed.album) return { tracks: [seed], startAt: 0 };
      const rows = await scan(fetch, {
        album: seed.album,
        ...(who ? { artist: who } : {}),
        mount: seed.mountId,
      });
      picked = byAlbum(withSeed(rows, seed), seed);
      break;
    }
    case 'artist': {
      if (!who) return { tracks: [seed], startAt: 0 };
      picked = byArtist(withSeed(await scan(fetch, { artist: who }), seed), seed);
      break;
    }
    case 'genre': {
      if (!seed.genres?.length) return { tracks: [seed], startAt: 0 };
      picked = byGenre(withSeed(await scan(fetch), seed), seed);
      break;
    }
    case 'similar': {
      const [byWho, everything] = await Promise.all([
        who ? scan(fetch, { artist: who }) : Promise.resolve([]),
        scan(fetch),
      ]);
      const pool = dedupe([seed, ...byWho, ...everything]);
      picked = { tracks: moreLikeThis(pool, seed), startAt: 0 };
      break;
    }
  }
  if (!opts.shuffle) return picked;
  return {
    tracks: shuffled(picked.tracks, opts.rng ?? seededRng(Date.now()), { first: seed.id }),
    startAt: 0,
  };
}

/** The seed is always part of the pool, even when the server page did not return it. */
function withSeed(rows: LibraryTrack[], seed: LibraryTrack): LibraryTrack[] {
  return rows.some((t) => t.id === seed.id) ? rows : [...rows, seed];
}
