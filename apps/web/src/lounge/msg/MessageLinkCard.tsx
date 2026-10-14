import { Avatar } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { formatDate, useT } from '../../i18n';
import { useAllChannels } from '../data';
import { useLoungeGotoMessage } from '../nav';

export interface MessageLinkRef {
  guildId: string;
  channelId: string;
  messageId: string;
}

/**
 * Message permalinks in this site's own address space: `https://this-host/channels/GUILD/CHANNEL/MESSAGE`
 * (the form Copy Message Link makes). At most `limit` distinct ones, in order of appearance.
 */
export function findMessageLinks(content: string, origin: string, limit = 2): MessageLinkRef[] {
  const out: MessageLinkRef[] = [];
  for (const url of content.match(/https?:\/\/[^\s<>]+/g) ?? []) {
    let u: URL;
    try {
      u = new URL(url);
    } catch {
      continue;
    }
    if (u.origin !== origin) continue;
    const m = /^\/channels\/(@me|[\w-]+)\/([\w-]+)\/([\w-]+)\/?$/.exec(u.pathname);
    if (!m) continue;
    const ref = { guildId: m[1]!, channelId: m[2]!, messageId: m[3]! };
    if (out.some((r) => r.messageId === ref.messageId)) continue;
    out.push(ref);
    if (out.length >= limit) break;
  }
  return out;
}

interface Linked {
  content?: string;
  timestamp?: string;
  author?: { username: string; global_name?: string | null };
  attachments?: unknown[];
}

/** Preview of a linked message (LNG-MSGUI-019): author, a short excerpt and where it lives. */
export function MessageLinkCard({ link }: { link: MessageLinkRef }) {
  const t = useT();
  const [msg, setMsg] = useState<Linked | null | 'denied'>(null);
  const channels = useAllChannels();
  const goto = useLoungeGotoMessage();
  const channel = (channels[link.guildId] ?? []).find((c) => c.id === link.channelId);

  useEffect(() => {
    let live = true;
    apiFetch<Linked>(`/channels/${link.channelId}/messages/${link.messageId}`, { retries: 0 })
      .then((m) => live && setMsg(m))
      .catch(() => live && setMsg('denied'));
    return () => {
      live = false;
    };
  }, [link.channelId, link.messageId]);

  // Not visible to the viewer, or deleted: no card, the link text stays.
  if (!msg || msg === 'denied') return null;
  const name =
    msg.author?.global_name || msg.author?.username || t('lounge.messageLinkCard.someone');
  const text =
    msg.content?.trim() ||
    (msg.attachments?.length ? t('lounge.messageLinkCard.clickToSeeAttachment') : '');
  return (
    <section
      className="lng-embed lng-msglink"
      aria-label={t('lounge.messageLinkCard.linkedFrom', { name })}
    >
      <div className="lng-embed-main">
        <div className="lng-embed-provider">
          {link.guildId === '@me'
            ? t('lounge.messageLinkCard.directMessage')
            : channel
              ? `#${channel.name}`
              : t('lounge.messageLinkCard.message')}
        </div>
        <div className="lng-msglink-head">
          <Avatar name={name} size={24} aria-hidden="true" />
          <strong>{name}</strong>
          {msg.timestamp && (
            <time className="lng-muted" dateTime={msg.timestamp}>
              {formatDate(new Date(msg.timestamp), {})}
            </time>
          )}
        </div>
        {text && <div className="lng-msglink-text">{text.slice(0, 280)}</div>}
        <button
          type="button"
          className="lng-link-btn"
          onClick={() => goto(link.guildId, link.channelId, link.messageId)}
        >
          {t('lounge.messageLinkCard.jumpToMessage')}
        </button>
      </div>
    </section>
  );
}
