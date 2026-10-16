import { Popover } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useStore } from '../../store';

/** Signed image URLs of custom emoji, one request per emoji per session (`null` = not usable here). */
const urls = new Map<string, Promise<string | null>>();
export const resetCustomEmojiUrls = () => urls.clear();

function loadUrl(guildId: string, id: string, size: number): Promise<string | null> {
  const key = `${guildId}:${id}:${size}`;
  let p = urls.get(key);
  if (!p) {
    p = apiFetch<{ url: string }>(`/guilds/${guildId}/emojis/${id}/url?size=${size}`, {
      retries: 0,
    }).then(
      (r) => r.url,
      () => {
        urls.delete(key);
        return null;
      },
    );
    urls.set(key, p);
  }
  return p;
}

function useEmojiUrl(
  guildId: string | undefined,
  id: string,
  size: number,
): string | null | undefined {
  const [url, setUrl] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    if (!guildId) {
      setUrl(null);
      return;
    }
    let live = true;
    void loadUrl(guildId, id, size).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [guildId, id, size]);
  return url;
}

export interface EmojiRef {
  id: string;
  name: string;
  animated: boolean;
}

function EmojiPopout({ guildId, emoji }: { guildId: string; emoji: EmojiRef }) {
  const t = useT();
  const big = useEmojiUrl(guildId, emoji.id, 96);
  const guildName = useStore((s) => s.guilds[guildId]?.name);
  return (
    <div className="lng-emoji-pop">
      {big ? (
        <img className="lng-emoji-pop-img" src={big} alt="" draggable={false} />
      ) : (
        <span className="lng-emoji-pop-img" aria-hidden="true" />
      )}
      <div className="lng-emoji-pop-text">
        <strong>{`:${emoji.name}:`}</strong>
        <span className="lng-emoji-pop-source">
          {guildName
            ? t('lounge.emojiPopout.from', { server: guildName })
            : t('lounge.emojiPopout.unknownServer')}
        </span>
      </div>
    </div>
  );
}

/**
 * A custom emoji inside a message. It is drawn from the signed URL of the server the message is in; clicking it
 * opens a popout with its name and source server (LNG-EXP-011). Emoji the viewer cannot resolve (another server's,
 * or a deleted one) stay as `:name:` text.
 */
export function CustomEmojiView({
  guildId,
  emoji,
}: {
  guildId: string | undefined;
  emoji: EmojiRef;
}) {
  const t = useT();
  const url = useEmojiUrl(guildId, emoji.id, 48);
  if (!guildId || !url) return <span className="sr-md-emoji-fallback">{`:${emoji.name}:`}</span>;
  return (
    <Popover
      label={t('lounge.emojiPopout.label', { name: emoji.name })}
      placement="top"
      trigger={
        <button
          type="button"
          className="lng-emoji-btn"
          aria-label={t('lounge.emojiPopout.open', { name: emoji.name })}
        >
          <img
            className="sr-md-emoji sr-md-custom-emoji"
            src={url}
            alt=""
            data-animated={emoji.animated ? 'true' : undefined}
            draggable={false}
          />
        </button>
      }
    >
      <EmojiPopout guildId={guildId} emoji={emoji} />
    </Popover>
  );
}

export const customEmojiResolver = (guildId: string | undefined) => (emoji: EmojiRef) => (
  <CustomEmojiView guildId={guildId} emoji={emoji} />
);
