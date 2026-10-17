import { Icon, type IconName } from '@sideroom/ui';
import { Fragment, useEffect, useState, type ReactNode } from 'react';
import { apiFetch } from '../../api/client';
import { t, useT } from '../../i18n';
import type { Message, Snowflake } from '../data';
import { formatMessageTime } from '../time';

/** Discord message types that are not authored chat (LNG-MSGUI-030). */
export const SYSTEM_CALL = 3;
export const SYSTEM_PIN = 6;
export const SYSTEM_JOIN = 7;
/** Server support contribution (Discord's boost types 8 to 11). */
export const SYSTEM_SUPPORT = [8, 9, 10, 11] as const;
export const SYSTEM_FOLLOW_ADD = 12;
export const SYSTEM_THREAD_CREATED = 18;
export const SYSTEM_THREAD_STARTER = 21;

/** AutoMod and incident alert cards (type 24) are the system user's messages with embeds and buttons; they render like one. */
export const SYSTEM_AUTOMOD_ACTION = 24;

/** The live watch party card (type 200) is the system user's message and draws as a card, not as a notice. */
const WATCH_PARTY_CARD = 200;

export const isSystemMessage = (m: Pick<Message, 'type'>) =>
  m.type !== undefined &&
  m.type !== 0 &&
  m.type !== WATCH_PARTY_CARD &&
  m.type !== 19 &&
  m.type !== 20 &&
  m.type !== 23 &&
  m.type !== SYSTEM_AUTOMOD_ACTION;

/**
 * Fills a catalog message that has `{name}`-style slots with React nodes: the message is formatted with
 * marker text for each slot, then split around the markers.
 */
function rich(key: string, parts: Record<string, ReactNode>): ReactNode {
  const values = Object.fromEntries(Object.keys(parts).map((k) => [k, `\uE000${k}\uE000`]));
  const pieces = t(key, values).split(/\uE000(\w+)\uE000/);
  return (
    <>
      {pieces.map((piece, i) =>
        i % 2 === 1 ? <Fragment key={i}>{parts[piece]}</Fragment> : piece,
      )}
    </>
  );
}

/** Member-join lines; the line for a join is picked from its id so it never changes between renders. */
export const JOIN_LINES: readonly ((name: ReactNode) => ReactNode)[] = [
  (n) => rich('lounge.systemMessage.join1', { name: n }),
  (n) => rich('lounge.systemMessage.join2', { name: n }),
  (n) => rich('lounge.systemMessage.join3', { name: n }),
  (n) => rich('lounge.systemMessage.join4', { name: n }),
  (n) => rich('lounge.systemMessage.join5', { name: n }),
  (n) => rich('lounge.systemMessage.join6', { name: n }),
  (n) => rich('lounge.systemMessage.join7', { name: n }),
  (n) => rich('lounge.systemMessage.join8', { name: n }),
  (n) => rich('lounge.systemMessage.join9', { name: n }),
  (n) => rich('lounge.systemMessage.join10', { name: n }),
  (n) => rich('lounge.systemMessage.join11', { name: n }),
  (n) => rich('lounge.systemMessage.join12', { name: n }),
  (n) => rich('lounge.systemMessage.join13', { name: n }),
];

/** Index of the join line for a message id (stable, spread evenly). */
export function joinLineIndex(id: string): number {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return h % JOIN_LINES.length;
}

export interface SystemMessageProps {
  message: Message;
  /** `message_reference.message_id` (the pinned message, the thread id, the original message). */
  referenceId?: Snowflake | null | undefined;
  /** `message_reference.channel_id`. */
  referenceChannelId?: Snowflake | null | undefined;
  onJumpTo?: ((id: Snowflake) => void) | undefined;
  onOpenThread?: ((threadId: Snowflake) => void) | undefined;
  /** Jump to a message in another channel (the original of a thread's first row). */
  onJumpToChannelMessage?: ((channelId: Snowflake, messageId: Snowflake) => void) | undefined;
}

/** Preview of the message a thread grew from, fetched once. */
function ThreadStarterPreview({
  channelId,
  messageId,
  onJump,
}: {
  channelId: Snowflake;
  messageId: Snowflake;
  onJump?: (() => void) | undefined;
}) {
  const t = useT();
  const [original, setOriginal] = useState<{ author: string; text: string } | null | 'gone'>(null);
  useEffect(() => {
    let live = true;
    apiFetch<{ content?: string; author?: { username: string; global_name?: string | null } }>(
      `/channels/${channelId}/messages/${messageId}`,
      { retries: 0 },
    )
      .then((m) => {
        if (live)
          setOriginal({
            author: m.author?.global_name || m.author?.username || '',
            text: m.content || '',
          });
      })
      .catch(() => live && setOriginal('gone'));
    return () => {
      live = false;
    };
  }, [channelId, messageId]);
  if (original === 'gone')
    return <span className="lng-muted">{t('lounge.systemMessage.threadGone')}</span>;
  return (
    <>
      <span className="lng-muted">{t('lounge.systemMessage.threadStartedFrom')}</span>
      {original ? (
        <>
          <strong className="lng-sys-name">
            {original.author || t('lounge.systemMessage.someone')}
          </strong>
          <q className="lng-sys-quote">
            {(original.text || t('lounge.systemMessage.attachment')).slice(0, 200)}
          </q>
        </>
      ) : (
        <span className="lng-muted">{t('lounge.systemMessage.aMessageEllipsis')}</span>
      )}
      {onJump && (
        <button type="button" className="lng-link-btn" onClick={onJump}>
          {' '}
          {t('lounge.systemMessage.jumpToOriginal')}
        </button>
      )}
    </>
  );
}

/** Rows for pins, joins, threads, calls, follows and server support. Anything else is a neutral line. */
export function SystemMessage({
  message: m,
  referenceId,
  referenceChannelId,
  onJumpTo,
  onOpenThread,
  onJumpToChannelMessage,
}: SystemMessageProps) {
  const t = useT();
  const name = <strong className="lng-sys-name">{m.author.displayName}</strong>;
  let icon: IconName = 'arrow-right';
  let body: ReactNode;
  switch (m.type) {
    case SYSTEM_PIN:
      icon = 'pin';
      body = rich('lounge.systemMessage.pinned', {
        name,
        message:
          referenceId && onJumpTo ? (
            <button type="button" className="lng-link-btn" onClick={() => onJumpTo(referenceId)}>
              {t('lounge.systemMessage.aMessage')}
            </button>
          ) : (
            t('lounge.systemMessage.aMessage')
          ),
      });
      break;
    case SYSTEM_JOIN:
      body = JOIN_LINES[joinLineIndex(m.id)]!(name);
      break;
    case SYSTEM_THREAD_CREATED:
      icon = 'list';
      body = rich('lounge.systemMessage.threadCreated', {
        name,
        thread:
          referenceId && onOpenThread ? (
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => onOpenThread(referenceId)}
            >
              {m.content || t('lounge.systemMessage.threadFallbackLink')}
            </button>
          ) : (
            <strong>{m.content || t('lounge.systemMessage.threadFallbackName')}</strong>
          ),
      });
      break;
    case SYSTEM_THREAD_STARTER:
      icon = 'list';
      body =
        referenceId && referenceChannelId ? (
          <ThreadStarterPreview
            channelId={referenceChannelId}
            messageId={referenceId}
            onJump={
              onJumpToChannelMessage
                ? () => onJumpToChannelMessage(referenceChannelId, referenceId)
                : undefined
            }
          />
        ) : (
          <span className="lng-muted">{t('lounge.systemMessage.threadStarterNoRef')}</span>
        );
      break;
    case SYSTEM_CALL:
      icon = 'volume';
      body = rich('lounge.systemMessage.call', { name });
      break;
    case SYSTEM_FOLLOW_ADD:
      icon = 'bell';
      body = rich('lounge.systemMessage.followAdd', {
        name,
        channel: <strong>{m.content || t('lounge.systemMessage.channelFallback')}</strong>,
      });
      break;
    default:
      if ((SYSTEM_SUPPORT as readonly number[]).includes(m.type ?? -1)) {
        icon = 'star';
        body = rich('lounge.systemMessage.supported', { name });
      } else body = <>{m.content || t('lounge.systemMessage.systemMessage')}</>;
  }
  return (
    <article
      className="lng-msg lng-system"
      data-message-id={m.id}
      data-system-type={m.type}
      aria-label={t('lounge.systemMessage.ariaLabel', { time: formatMessageTime(m.createdAt) })}
    >
      <div className="lng-msg-main">
        <div className="lng-msg-gutter">
          <Icon name={icon} size={20} />
        </div>
        <div className="lng-msg-body">
          <span>{body}</span>{' '}
          <time className="lng-msg-time" dateTime={m.createdAt}>
            {formatMessageTime(m.createdAt)}
          </time>
        </div>
      </div>
    </article>
  );
}
