import { Tooltip } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import type { StickerItem } from '../data';

const cache = new Map<string, Promise<string | null>>();

/** Signed CDN URL of a guild sticker (cached for the session; null when it cannot be resolved). */
export function stickerUrl(guildId: string, id: string): Promise<string | null> {
  const key = `${guildId}:${id}`;
  let p = cache.get(key);
  if (!p) {
    p = apiFetch<{ url: string }>(`/guilds/${guildId}/stickers/${id}/url?size=256`, {
      retries: 0,
    })
      .then((r) => r.url)
      .catch(() => {
        cache.delete(key);
        return null;
      });
    cache.set(key, p);
  }
  return p;
}

/** Test hook. */
export const resetStickerCache = () => cache.clear();

/** A sticker in a message (LNG-MSGUI-020): PNG/APNG/GIF render as images, with the name as tooltip. */
export function Sticker({
  sticker,
  guildId,
}: {
  sticker: StickerItem;
  guildId: string | undefined;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    if (guildId) void stickerUrl(guildId, sticker.id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [guildId, sticker.id]);
  return (
    <Tooltip content={sticker.name}>
      <span className="lng-sticker" tabIndex={0}>
        {url ? (
          <img src={url} alt={`Sticker: ${sticker.name}`} width={160} height={160} />
        ) : (
          <span className="lng-sticker-ph" role="img" aria-label={`Sticker: ${sticker.name}`} />
        )}
      </span>
    </Tooltip>
  );
}
