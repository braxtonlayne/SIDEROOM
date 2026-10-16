import { useEffect, useState } from 'react';

/** How often a visible, unwatched stream tile asks for a fresher thumbnail (the streamer uploads every 30 s). */
export const PREVIEW_REFRESH_MS = 30_000;

export type PreviewFetcher = (key: string, etag: string | null) => Promise<Response>;

const browserFetcher: PreviewFetcher = (key, etag) =>
  fetch(`/api/v1/streams/${encodeURIComponent(key)}/preview`, {
    credentials: 'include',
    ...(etag ? { headers: { 'If-None-Match': etag } } : {}),
  });

let fetcher: PreviewFetcher = browserFetcher;
/** Tests: replace how the thumbnail is fetched. */
export function setPreviewFetcher(f: PreviewFetcher | null): void {
  fetcher = f ?? browserFetcher;
}

/**
 * The newest thumbnail of a stream (VOX-CLI-008) as an object URL, or null while there is none (the streamer has
 * not uploaded yet, or the viewer may not see it). Refreshes on a timer while `enabled`; revokes its URLs.
 */
export function useStreamPreview(key: string | null, enabled: boolean): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!key || !enabled) {
      setUrl(null);
      return;
    }
    let dead = false;
    let current: string | null = null;
    let etag: string | null = null;
    const load = async () => {
      try {
        const res = await fetcher(key, etag);
        if (dead || res.status === 304) return;
        if (!res.ok) {
          if (res.status === 404 && current) {
            URL.revokeObjectURL(current);
            current = null;
            setUrl(null);
          }
          return;
        }
        const blob = await res.blob();
        if (dead) return;
        const next = URL.createObjectURL(blob);
        if (current) URL.revokeObjectURL(current);
        current = next;
        etag = res.headers.get('etag');
        setUrl(next);
      } catch {
        /* offline: keep what we have */
      }
    };
    void load();
    const timer = setInterval(() => void load(), PREVIEW_REFRESH_MS);
    return () => {
      dead = true;
      clearInterval(timer);
      if (current) URL.revokeObjectURL(current);
    };
  }, [key, enabled]);
  return url;
}
