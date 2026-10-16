// Custom emoji in message text (LNG-EXP-011, D-232): the image comes from `GET /sr/emojis/{id}/url` (any session, so
// emoji from servers the viewer is not in still draw), and a click opens a popout with the name, the source server
// and, when that server is in the Directory, a Join button (`POST /directory/{listing}/join`).
import { Avatar, Button, Notice, Popover } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { cdnUrl, useStore } from '../../store';
import { joinListedGuild } from '../shell/directory';

export interface EmojiInfo {
  id: string;
  name: string;
  animated: boolean;
  guild: { id: string; name: string; icon?: string | null; listing?: string | null } | null;
}

const urls = new Map<string, Promise<string | null>>();
/** The signed image URL of a custom emoji; null when it cannot be resolved (shows `:name:`). */
export function fetchEmojiUrl(id: string): Promise<string | null> {
  let p = urls.get(id);
  if (!p) {
    p = apiFetch<{ url: string }>(`/sr/emojis/${encodeURIComponent(id)}/url?size=64`, {
      retries: 0,
    })
      .then((r) => r.url)
      .catch(() => {
        urls.delete(id);
        return null;
      });
    urls.set(id, p);
  }
  return p;
}
/** Test hook. */
export const resetEmojiUrls = () => urls.clear();

export const fetchEmojiInfo = (id: string) =>
  apiFetch<EmojiInfo>(`/sr/emojis/${encodeURIComponent(id)}`, { retries: 0 });

function useEmojiUrl(id: string): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void fetchEmojiUrl(id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [id]);
  return url;
}

function EmojiPopout({ id, url }: { id: string; url: string | null }) {
  const t = useT();
  const [info, setInfo] = useState<EmojiInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [joined, setJoined] = useState(false);
  const [busy, setBusy] = useState(false);
  const memberOf = useStore((s) => (info?.guild ? info.guild.id in s.guilds : false));
  useEffect(() => {
    let live = true;
    fetchEmojiInfo(id)
      .then((i) => live && setInfo(i))
      .catch(() => live && setError(t('lounge.emojiPopout.loadError')));
    return () => {
      live = false;
    };
  }, [id, t]);
  if (error) return <Notice variant="danger">{error}</Notice>;
  if (!info)
    return (
      <p className="lng-emoji-popout-loading" role="status">
        {t('lounge.emojiPopout.loading')}
      </p>
    );
  const g = info.guild;
  const icon = g ? cdnUrl('icons', g.id, g.icon, { size: 64 }) : null;
  const join = async () => {
    if (!g?.listing) return;
    setBusy(true);
    setError(null);
    try {
      await joinListedGuild(g.listing);
      setJoined(true);
    } catch {
      setError(t('lounge.emojiPopout.joinError'));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="lng-emoji-popout">
      <div className="lng-emoji-popout-head">
        {url && <img src={url} alt="" width={48} height={48} />}
        <strong>{`:${info.name}:`}</strong>
      </div>
      {g ? (
        <div className="lng-emoji-popout-source">
          <Avatar name={g.name} size={32} {...(icon ? { src: icon } : {})} />
          <span>{t('lounge.emojiPopout.fromServer', { name: g.name })}</span>
        </div>
      ) : (
        <p className="lng-emoji-popout-private">{t('lounge.emojiPopout.privateServer')}</p>
      )}
      {g && memberOf && <p>{t('lounge.emojiPopout.member')}</p>}
      {g?.listing && !memberOf && !joined && (
        <Button size="sm" loading={busy} onClick={() => void join()}>
          {t('lounge.emojiPopout.join')}
        </Button>
      )}
      {joined && <p role="status">{t('lounge.emojiPopout.joined', { name: g?.name ?? '' })}</p>}
      {g && !g.listing && !memberOf && <p>{t('lounge.emojiPopout.notListed')}</p>}
    </div>
  );
}

/** A custom emoji in a message: the image as a button that opens the popout. */
export function CustomEmoji({ id, name }: { id: string; name: string; animated?: boolean }) {
  const t = useT();
  const url = useEmojiUrl(id);
  return (
    <Popover
      label={t('lounge.emojiPopout.label', { name })}
      placement="top"
      trigger={
        <button
          type="button"
          className="lng-custom-emoji"
          aria-label={t('lounge.emojiPopout.open', { name })}
        >
          {url ? (
            <img
              className="sr-md-emoji sr-md-custom-emoji"
              src={url}
              alt={`:${name}:`}
              draggable={false}
            />
          ) : (
            <span className="sr-md-emoji-fallback">{`:${name}:`}</span>
          )}
        </button>
      }
    >
      <EmojiPopout id={id} url={url} />
    </Popover>
  );
}
