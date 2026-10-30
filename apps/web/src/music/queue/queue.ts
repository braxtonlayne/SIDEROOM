/**
 * MUS-LIB-001: the smart queue rules. Pure functions over plain track records: which tracks make "the album",
 * "the artist", "the genre" and "more like this", and how a queue is shuffled without repeats. Nothing here
 * touches the player: the panel builds a list with these and hands it to the player's `play` / `enqueue`.
 */

/** The fields of a library track the rules look at (a subset of `GET /music/library` items). */
export interface QueueTrack {
  id: string;
  title: string;
  artist?: string;
  album?: string;
  albumArtist?: string;
  genres?: readonly string[];
  tags?: readonly string[];
  year?: number | null;
  trackNo?: number | null;
  discNo?: number | null;
}

/** A source of randomness in [0, 1); tests pass a seeded one. */
export type Rng = () => number;

const norm = (s: string | undefined): string => (s ?? '').trim().toLowerCase();

/** Lower-cased, trimmed, empties dropped, duplicates dropped. */
function terms(list: readonly string[] | undefined): Set<string> {
  const out = new Set<string>();
  for (const x of list ?? []) {
    const n = norm(x);
    if (n) out.add(n);
  }
  return out;
}

/** The artist a track is filed under: the album artist when there is one, else the artist. */
const owner = (t: QueueTrack): string => norm(t.albumArtist) || norm(t.artist);

/** Album order: disc, track number (tracks without a number last), then title. */
export function albumOrder(a: QueueTrack, b: QueueTrack): number {
  const disc = (a.discNo ?? 1) - (b.discNo ?? 1);
  if (disc !== 0) return disc;
  const an = a.trackNo ?? Number.MAX_SAFE_INTEGER;
  const bn = b.trackNo ?? Number.MAX_SAFE_INTEGER;
  if (an !== bn) return an < bn ? -1 : 1;
  return a.title.localeCompare(b.title);
}

/** A list with the index to start at (the seed's place in it). */
export interface Picked<T extends QueueTrack> {
  tracks: T[];
  startAt: number;
}

function startOf<T extends QueueTrack>(tracks: T[], seed: QueueTrack): Picked<T> {
  const i = tracks.findIndex((t) => t.id === seed.id);
  return { tracks, startAt: i < 0 ? 0 : i };
}

/** The seed's album, in album order. Tracks match on album name and the same album artist or artist. */
export function byAlbum<T extends QueueTrack>(all: readonly T[], seed: QueueTrack): Picked<T> {
  const album = norm(seed.album);
  const who = owner(seed);
  const tracks = all
    .filter((t) => album !== '' && norm(t.album) === album && owner(t) === who)
    .sort(albumOrder);
  return tracks.some((t) => t.id === seed.id)
    ? startOf(tracks, seed)
    : { tracks: [seed as T], startAt: 0 };
}

/** Everything by the seed's artist: by year, then album, then album order. */
export function byArtist<T extends QueueTrack>(all: readonly T[], seed: QueueTrack): Picked<T> {
  const who = owner(seed);
  const tracks = all
    .filter((t) => who !== '' && (owner(t) === who || norm(t.artist) === norm(seed.artist)))
    .sort((a, b) => {
      const y = (a.year ?? 9999) - (b.year ?? 9999);
      if (y !== 0) return y;
      const al = norm(a.album).localeCompare(norm(b.album));
      return al !== 0 ? al : albumOrder(a, b);
    });
  return tracks.some((t) => t.id === seed.id)
    ? startOf(tracks, seed)
    : { tracks: [seed as T], startAt: 0 };
}

/** Everything that shares a genre with the seed (any of them), in artist, album order. */
export function byGenre<T extends QueueTrack>(all: readonly T[], seed: QueueTrack): Picked<T> {
  const g = terms(seed.genres);
  const tracks = all
    .filter((t) => [...terms(t.genres)].some((x) => g.has(x)))
    .sort((a, b) => {
      const ar = owner(a).localeCompare(owner(b));
      if (ar !== 0) return ar;
      const al = norm(a.album).localeCompare(norm(b.album));
      return al !== 0 ? al : albumOrder(a, b);
    });
  return tracks.some((t) => t.id === seed.id)
    ? startOf(tracks, seed)
    : { tracks: [seed as T], startAt: 0 };
}

/** Weights of "more like this". Tags say the most, then genre, then who made it, then when. */
export const SIMILARITY = { tag: 4, genre: 3, artist: 2, albumArtist: 1, era: 1 } as const;

/** How alike two tracks are (0 = nothing in common). Symmetric. */
export function similarity(a: QueueTrack, b: QueueTrack): number {
  let score = 0;
  const ta = terms(a.tags);
  for (const x of terms(b.tags)) if (ta.has(x)) score += SIMILARITY.tag;
  const ga = terms(a.genres);
  for (const x of terms(b.genres)) if (ga.has(x)) score += SIMILARITY.genre;
  if (norm(a.artist) !== '' && norm(a.artist) === norm(b.artist)) score += SIMILARITY.artist;
  else if (owner(a) !== '' && owner(a) === owner(b)) score += SIMILARITY.albumArtist;
  if (score > 0 && a.year && b.year && Math.abs(a.year - b.year) <= 5) score += SIMILARITY.era;
  return score;
}

/**
 * "More like this": the seed first, then the candidates that share something with it, most alike first. Ties
 * keep the candidates' own order, so the result does not jump around between calls. Zero-score tracks are not
 * "like" anything and are left out.
 */
export function moreLikeThis<T extends QueueTrack>(
  candidates: readonly T[],
  seed: T,
  limit = 50,
): T[] {
  const scored = candidates
    .map((t, order) => ({ t, order, score: t.id === seed.id ? 0 : similarity(seed, t) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .slice(0, Math.max(0, limit - 1))
    .map((x) => x.t);
  return [seed, ...scored];
}

/** A small seeded generator (mulberry32) so a shuffle can be reproduced in tests. */
export function seededRng(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Fisher-Yates over a copy. */
function permute<T>(list: readonly T[], rng: Rng): T[] {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [a[i], a[j]] = [a[j]!, a[i]!];
  }
  return a;
}

/**
 * Shuffle with no repeat: a random permutation, so every track plays once before any plays again. `first` (a
 * track id) is put at the front, which is how "shuffle this album, starting with this song" works. With
 * `avoidFirst` the permutation does not open with that id (the start of a new cycle must not repeat the track
 * that just played), unless it is the only track.
 */
export function shuffled<T extends QueueTrack>(
  tracks: readonly T[],
  rng: Rng = Math.random,
  opts: { first?: string; avoidFirst?: string } = {},
): T[] {
  if (tracks.length <= 1) return [...tracks];
  const head = opts.first !== undefined ? tracks.find((t) => t.id === opts.first) : undefined;
  if (head) {
    const rest = permute(
      tracks.filter((t) => t !== head),
      rng,
    );
    return [head, ...rest];
  }
  const out = permute(tracks, rng);
  if (opts.avoidFirst !== undefined && out[0]?.id === opts.avoidFirst) {
    // Swap the opener with a random other slot: still uniform enough, and never the same opener.
    const j = 1 + Math.floor(rng() * (out.length - 1));
    [out[0], out[j]] = [out[j]!, out[0]!];
  }
  return out;
}

/** The queue with its current index. */
export interface QueueView<T> {
  tracks: T[];
  index: number;
}

/**
 * Shuffle what is still to come, keeping what has played and the current track where they are, so turning
 * shuffle on mid-album does not restart or skip the song that is playing.
 */
export function shuffleUpcoming<T extends QueueTrack>(
  q: QueueView<T>,
  rng: Rng = Math.random,
): QueueView<T> {
  const keep = Math.max(0, q.index + 1);
  const head = q.tracks.slice(0, keep);
  const tail = q.tracks.slice(keep);
  return { tracks: [...head, ...permute(tail, rng)], index: q.index };
}

/**
 * The next cycle of a repeating shuffle: everything again in a new order that does not open with the track
 * that just ended.
 */
export function nextCycle<T extends QueueTrack>(
  tracks: readonly T[],
  lastPlayedId: string,
  rng: Rng = Math.random,
): T[] {
  return shuffled(tracks, rng, { avoidFirst: lastPlayedId });
}

/** Move a track within the queue; the current index follows the track that is playing. */
export function moveTrack<T>(q: QueueView<T>, from: number, to: number): QueueView<T> {
  const n = q.tracks.length;
  if (from < 0 || from >= n || to < 0 || to >= n || from === to) return q;
  const tracks = [...q.tracks];
  const [moved] = tracks.splice(from, 1);
  tracks.splice(to, 0, moved as T);
  let index = q.index;
  if (q.index === from) index = to;
  else if (from < q.index && to >= q.index) index = q.index - 1;
  else if (from > q.index && to <= q.index) index = q.index + 1;
  return { tracks, index };
}

/** Drop later copies of a track id (a queue built from two sources may carry the same song twice). */
export function dedupe<T extends QueueTrack>(tracks: readonly T[]): T[] {
  const seen = new Set<string>();
  return tracks.filter((t) => (seen.has(t.id) ? false : (seen.add(t.id), true)));
}

/** Total play time in seconds of the tracks that know their length. */
export function totalSeconds(tracks: readonly { durationMs?: number | null }[]): number {
  return tracks.reduce((s, t) => s + (t.durationMs ? t.durationMs / 1000 : 0), 0);
}
