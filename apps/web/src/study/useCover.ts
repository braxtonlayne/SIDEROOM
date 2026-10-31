import { useEffect, useRef, useState } from 'react';
import { presignGet } from '../storage/api';
import { NATIVE_VIDEO } from '../storage/shelves/api';
import { CoverQueue, coverSource, extractCover, type CoverDeps } from './covers';
import { extOf } from './fileTypes';

let tags: ReturnType<typeof loadTags> | null = null;
let lister: ReturnType<typeof loadLister> | null = null;
const loadTags = () => import('@sideroom/wasm').then((m) => m.loadMusic());
const loadLister = () =>
  import('@sideroom/wasm').then(async (m) => {
    const api = await m.loadLibrary();
    return { firstTail: api.firstTail, listPages: api.listPages };
  });

/** The wasm facades, loaded on the first cover that needs them. */
export const wasmDeps: CoverDeps = {
  tags: () => (tags ??= loadTags()),
  lister: () => (lister ??= loadLister()),
};

/** One queue for the page: two extractions at a time, each file once. */
export const coverQueue = new CoverQueue(2);

/** True once the element has come near the viewport (always true where IntersectionObserver is missing). */
export function useNearViewport<T extends Element>() {
  const ref = useRef<T>(null);
  const [near, setNear] = useState(typeof IntersectionObserver === 'undefined');
  useEffect(() => {
    const el = ref.current;
    if (near || !el) return;
    const io = new IntersectionObserver(
      (es) => {
        if (es.some((e) => e.isIntersecting)) {
          setNear(true);
          io.disconnect();
        }
      },
      { rootMargin: '200px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [near]);
  return { ref, near };
}

/**
 * The cover URL of a file, or null. The owner's chosen cover (an image on the same mount) wins; otherwise the cover
 * comes from the file itself (`extractCover`). Nothing leaves the viewer's browser except the range reads of the
 * file's own presigned URL.
 */
export function useCover(
  mountId: string,
  key: string,
  size: number,
  ownerCover: string | undefined,
  enabled: boolean,
): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const src = coverSource(key);
    const direct = ownerCover ?? (src === 'image' ? key : null);
    if (direct) {
      presignGet(mountId, direct)
        .then((p) => alive && setUrl(p.url))
        .catch(() => undefined);
      return () => {
        alive = false;
      };
    }
    if (!src) return;
    // A browser cannot decode an MKV or AVI: no frame to capture.
    if (src === 'video' && !NATIVE_VIDEO.has(extOf(key))) return;
    coverQueue
      .get(`${mountId}:${key}:${size}`, async () => {
        const signed = await presignGet(mountId, key);
        return extractCover(key, signed.url, size, wasmDeps);
      })
      .then((u) => alive && setUrl(u))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [mountId, key, size, ownerCover, enabled]);
  return url;
}
