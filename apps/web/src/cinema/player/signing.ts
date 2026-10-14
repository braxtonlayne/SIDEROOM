// Signed URL refresh without a stall (CIN-PLY-010). Every URL in a rendered playlist carries its own signature
// (`path?ex=..&sig=..`, sr-media playlist.rs) and expires together, six hours after the grant. hls.js loads the
// playlists once and then keeps requesting segments from them, so near expiry the player asks for a fresh grant,
// reads the new playlists, and builds a map from each file's path to its freshly signed URL. From then on every
// request hls.js makes is rewritten through that map: the stream never restarts and nothing rebuffers.

export type FetchText = (url: string) => Promise<string>;
export type UrlRewriter = (url: string) => string;

const defaultFetch: FetchText = async (url) => {
  const res = await fetch(url, { credentials: 'include' });
  if (!res.ok) throw new Error(`playlist ${res.status}`);
  return res.text();
};

/** Every URI a playlist mentions: bare lines and `URI="..."` attributes, resolved against the playlist. */
export function playlistRefs(text: string, base: string): string[] {
  const out: string[] = [];
  const add = (raw: string) => {
    if (!raw || raw.startsWith('data:')) return;
    try {
      out.push(new URL(raw, base).toString());
    } catch {
      /* not a URL: ignore */
    }
  };
  for (const line of text.replace(/\r/g, '').split('\n')) {
    const l = line.trim();
    if (!l) continue;
    if (l.startsWith('#')) {
      for (const m of l.matchAll(/URI="([^"]*)"/g)) add(m[1]!);
    } else add(l);
  }
  return out;
}

const keyOf = (url: string) => {
  const u = new URL(url, 'http://x.invalid');
  return `${u.origin}${u.pathname}`;
};
const isPlaylist = (url: string) => new URL(url, 'http://x.invalid').pathname.endsWith('.m3u8');

/**
 * Walks the playlists under a fresh master URL (at most `maxPlaylists`, all fetched at most twice-deep) and
 * returns a rewriter that swaps any known file for its freshly signed URL and leaves everything else alone.
 */
export async function loadSigningMap(
  masterUrl: string,
  fetchText: FetchText = defaultFetch,
  maxPlaylists = 64,
): Promise<UrlRewriter> {
  const fresh = new Map<string, string>();
  const seen = new Set<string>();
  let queue = [masterUrl];
  let fetched = 0;
  while (queue.length && fetched < maxPlaylists) {
    const batch = queue.splice(0, 6);
    const next: string[] = [];
    await Promise.all(
      batch.map(async (url) => {
        if (seen.has(keyOf(url))) return;
        seen.add(keyOf(url));
        fresh.set(keyOf(url), url);
        fetched += 1;
        const text = await fetchText(url);
        for (const ref of playlistRefs(text, url)) {
          fresh.set(keyOf(ref), ref);
          if (isPlaylist(ref)) next.push(ref);
        }
      }),
    );
    queue = [...queue, ...next];
  }
  return (url) => {
    try {
      return fresh.get(keyOf(url)) ?? url;
    } catch {
      return url;
    }
  };
}
