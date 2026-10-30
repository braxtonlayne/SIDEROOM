import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { presignGet } from '../../storage/api';
import type { ImgRef } from './model';

/** Presigned links are per viewer and short-lived: kept in memory only, dropped a little before they expire. */
const links = new Map<string, { url: string; until: number }>();
const pending = new Map<string, Promise<string>>();
const SKEW_MS = 30_000;

/** Forget every link (tests, sign-out). */
export function clearImageLinks(): void {
  links.clear();
  pending.clear();
}

/**
 * A link to a mount image for THIS viewer, asked from the storage presign endpoint, which applies the storage
 * scopes (own bucket, friend share, server grant). A room stores only `{mount, key}`, so someone without
 * access gets a refusal here, not the picture.
 */
export function mountImageUrl(mount: string, key: string): Promise<string> {
  const id = `${mount}\u0000${key}`;
  const hit = links.get(id);
  if (hit && hit.until > Date.now()) return Promise.resolve(hit.url);
  let p = pending.get(id);
  if (!p) {
    p = presignGet(mount, key)
      .then((r) => {
        const until = Date.parse(r.expires_at) - SKEW_MS;
        links.set(id, { url: r.url, until: Number.isFinite(until) ? until : Date.now() + 60_000 });
        return r.url;
      })
      .finally(() => pending.delete(id));
    pending.set(id, p);
  }
  return p;
}

type State = { url: string } | 'denied' | 'loading';

/** One image of a room; falls back to a labelled placeholder when this viewer cannot see it. */
export function RoomImage({ img, label }: { img: ImgRef; label: string }) {
  const t = useT();
  const [state, setState] = useState<State>(img.k === 'path' ? { url: img.path } : 'loading');
  useEffect(() => {
    if (img.k === 'path') {
      setState({ url: img.path });
      return;
    }
    let dead = false;
    setState('loading');
    mountImageUrl(img.mount, img.key).then(
      (url) => !dead && setState({ url }),
      () => !dead && setState('denied'),
    );
    return () => {
      dead = true;
    };
  }, [img]);
  if (state === 'loading')
    return <span className="sr-room-img sr-room-img-wait" aria-hidden="true" />;
  if (state === 'denied')
    return (
      <span
        className="sr-room-img sr-room-img-denied"
        role="img"
        aria-label={t('tools.rooms.imageDenied')}
      >
        ?
      </span>
    );
  return (
    <img className="sr-room-img" src={state.url} alt={label} loading="lazy" draggable={false} />
  );
}
