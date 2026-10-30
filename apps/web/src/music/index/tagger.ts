// The lazy tagger (MUS-IDX-001): reads the tags of tracks whose tags are still pending, in the background,
// 4 tracks at a time, and writes them with `PATCH /music/tracks/{id}`. The server never opens a file; this
// reads the first 1 MiB (or the tail) of each by a presigned range, parses it with the `sr-music-wasm` facade,
// stores cover art as `covers/<hash>.jpg` in the same mount and reports back.
import { withBackoff, type BackoffOptions } from './backoff';
import { coverHash, coverKey, toJpeg } from './cover';
import { runPool, TAG_CONCURRENCY } from './pool';
import { readTrackTags, type ReadOutcome } from './readTrack';
import type { Fetch } from './ranges';
import type { MusicTags, TagPatch, TaggerApi, TaggerProgress, TagReader, TrackRow } from './types';

const PAGE = 50;

export interface TaggerOptions {
  api: TaggerApi;
  /** Loads the facade (once). */
  reader: () => Promise<TagReader>;
  fetchFn?: Fetch;
  concurrency?: number;
  backoff?: Pick<BackoffOptions, 'attempts' | 'baseMs' | 'maxMs' | 'sleep' | 'random'>;
  /** JPEG bytes of a cover; a seam for tests. */
  toJpeg?: typeof toJpeg;
  hash?: typeof coverHash;
  warn?: (message: string) => void;
}

/** What a tag read becomes on the wire. Null fields are left out so a hand edit or a file name is not wiped. */
export function patchFrom(tags: MusicTags, cover: string | null): TagPatch {
  const p: TagPatch = { source: 'file', tags_state: 'read' };
  if (tags.title) p.title = tags.title;
  if (tags.artist) p.artist = tags.artist;
  if (tags.album) p.album = tags.album;
  if (tags.albumArtist) p.album_artist = tags.albumArtist;
  if (tags.track !== null && tags.track >= 0) p.track_no = tags.track;
  if (tags.disc !== null && tags.disc >= 0) p.disc_no = tags.disc;
  if (tags.year !== null && tags.year >= 1000 && tags.year <= 2200) p.year = tags.year;
  if (tags.durationHint !== null && tags.durationHint >= 0)
    p.duration_ms = Math.round(tags.durationHint);
  if (tags.genre) {
    const genres = tags.genre
      .split(/[;/\0]/)
      .map((g) => g.trim())
      .filter(Boolean);
    if (genres.length > 0) p.genres = genres;
  }
  if (cover) p.cover_key = cover;
  return p;
}

export class Tagger {
  private subs = new Set<() => void>();
  private state: TaggerProgress = { total: 0, done: 0, tagged: 0, failed: 0, running: false };
  private abort: AbortController | null = null;
  private running: Promise<TaggerProgress> | null = null;
  private warned = new Set<string>();
  private uploaded = new Map<string, Promise<void>>();
  private reader: Promise<TagReader> | null = null;

  constructor(private o: TaggerOptions) {}

  subscribe(fn: () => void): () => void {
    this.subs.add(fn);
    return () => this.subs.delete(fn);
  }

  /** Stable between changes (for `useSyncExternalStore`). */
  snapshot(): TaggerProgress {
    return this.state;
  }

  private set(p: Partial<TaggerProgress>) {
    this.state = { ...this.state, ...p };
    for (const f of this.subs) f();
  }

  private warnOnce = (reason: string, detail: string) => {
    if (this.warned.has(reason)) return;
    this.warned.add(reason);
    (this.o.warn ?? console.warn)(`music tagger: ${reason}: ${detail}`);
  };

  /** Tag everything pending (of one mount, or of every reachable one). Joins a run already going. */
  start(mountId?: string): Promise<TaggerProgress> {
    if (this.running) return this.running;
    this.abort = new AbortController();
    this.state = { total: 0, done: 0, tagged: 0, failed: 0, running: true };
    this.set({});
    const finish = () => {
      this.running = null;
      this.set({ running: false });
      return this.state;
    };
    this.running = this.run(mountId, this.abort.signal).then(finish, (e) => {
      finish();
      throw e;
    });
    return this.running;
  }

  /** "Rescan tags": the server marks the mount's tracks pending again, then the tagger reads them. */
  async rescan(mountId: string): Promise<TaggerProgress> {
    await this.running?.catch(() => undefined);
    await this.o.api.rescan(mountId);
    return this.start(mountId);
  }

  stop() {
    this.abort?.abort();
  }

  private async run(mountId: string | undefined, signal: AbortSignal): Promise<TaggerProgress> {
    const seen = new Set<string>();
    let offset = 0; // tracks that stayed pending (failed) in this run are skipped by offset
    while (!signal.aborted) {
      const page = await withBackoff(() => this.o.api.listPending(mountId, PAGE, offset), {
        ...this.o.backoff,
        signal,
      });
      const fresh = page.items.filter((t) => !seen.has(t.id));
      if (fresh.length === 0) break;
      for (const t of fresh) seen.add(t.id);
      this.set({ total: this.state.total + fresh.length });
      const failedBefore = this.state.failed;
      await runPool(fresh, (t) => this.one(t, signal), {
        concurrency: this.o.concurrency ?? TAG_CONCURRENCY,
        signal,
      });
      offset += this.state.failed - failedBefore;
      if (page.items.length < PAGE) break;
    }
    return this.state;
  }

  private async one(t: TrackRow, signal: AbortSignal): Promise<void> {
    try {
      const patch = await withBackoff(() => this.tagsOf(t), { ...this.o.backoff, signal });
      await withBackoff(() => this.o.api.patch(t.id, patch), { ...this.o.backoff, signal });
      this.set({ done: this.state.done + 1, tagged: this.state.tagged + 1 });
    } catch (e) {
      if (signal.aborted) return;
      this.warnOnce('track-failed', e instanceof Error ? e.message : String(e));
      // Leave a mark so "Rescan tags" and the next start know it needs another go. Best effort.
      await this.o.api.patch(t.id, { source: 'file', tags_state: 'failed' }).catch(() => undefined);
      this.set({ done: this.state.done + 1, failed: this.state.failed + 1 });
    }
  }

  private async tagsOf(t: TrackRow): Promise<TagPatch> {
    this.reader ??= this.o.reader();
    const reader = await this.reader;
    const { url } = await this.o.api.presign(t.id);
    const out: ReadOutcome = await readTrackTags(url, t.size, t.container, {
      reader,
      fetchFn: this.o.fetchFn ?? ((u, i) => fetch(u, i)),
      warnOnce: this.warnOnce,
    });
    if (out.kind === 'none') return { source: 'file', tags_state: 'none' };
    const cover = await this.storeCover(t, out.tags);
    return patchFrom(out.tags, cover);
  }

  /** Upload the picture (once per hash per mount) and return its key; a failure only loses the cover. */
  private async storeCover(t: TrackRow, tags: MusicTags): Promise<string | null> {
    if (!tags.cover || tags.cover.bytes.length === 0) return null;
    try {
      const jpeg = await (this.o.toJpeg ?? toJpeg)(tags.cover);
      const hash = await (this.o.hash ?? coverHash)(jpeg);
      const key = coverKey(hash);
      const mark = `${t.mount_id}/${hash}`;
      let up = this.uploaded.get(mark);
      if (!up) {
        up = this.o.api.uploadCover(
          t.mount_id,
          key,
          new Blob([jpeg as BlobPart], { type: 'image/jpeg' }),
        );
        this.uploaded.set(mark, up);
        // A failed upload is tried again by the next track with the same picture.
        up.catch(() => this.uploaded.delete(mark));
      }
      await up;
      return key;
    } catch (e) {
      this.warnOnce('cover-failed', e instanceof Error ? e.message : String(e));
      return null;
    }
  }
}

let shared: Tagger | null = null;

/** The app-wide tagger. */
export function getTagger(): Tagger {
  shared ??= new Tagger({
    api: realApi(),
    reader: loadReader,
  });
  return shared;
}

export function setTagger(t: Tagger | null) {
  shared = t;
}

function realApi(): TaggerApi {
  // Lazy: keeps the storage upload manager out of the module graph until a tag read starts.
  const api = () => import('./api').then((m) => m.realTaggerApi);
  return {
    listPending: async (m, l, o) => (await api()).listPending(m, l, o),
    presign: async (id) => (await api()).presign(id),
    patch: async (id, b) => (await api()).patch(id, b),
    rescan: async (m) => (await api()).rescan(m),
    uploadCover: async (m, k, b) => (await api()).uploadCover(m, k, b),
  };
}

/** The `sr-music-wasm` facade, loaded on first use (a dynamic import, so it stays out of the main bundle). */
async function loadReader(): Promise<TagReader> {
  const mod = (await import('@sideroom/wasm')) as unknown as {
    loadMusic?: () => Promise<TagReader>;
  };
  if (!mod.loadMusic) throw new Error('the music tag reader is not available in this build');
  return mod.loadMusic();
}
