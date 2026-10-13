import { Icon, type IconName } from '@sideroom/ui';
import { useEffect, useState, type ReactNode } from 'react';
import { apiFetch } from '../../api/client';
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

export const isSystemMessage = (m: Pick<Message, 'type'>) =>
  m.type !== undefined && m.type !== 0 && m.type !== 19 && m.type !== 20 && m.type !== 23;

/** Member-join lines; the line for a join is picked from its id so it never changes between renders. */
export const JOIN_LINES: readonly ((name: ReactNode) => ReactNode)[] = [
  (n) => <>{n} joined the party.</>,
  (n) => <>{n} is here.</>,
  (n) => <>Welcome, {n}. We hope you brought pizza.</>,
  (n) => <>A wild {n} appeared.</>,
  (n) => <>{n} just landed.</>,
  (n) => <>{n} just slid into the server.</>,
  (n) => <>Everyone welcome {n}!</>,
  (n) => <>Glad you&apos;re here, {n}.</>,
  (n) => <>Good to see you, {n}.</>,
  (n) => <>Yay you made it, {n}!</>,
  (n) => <>{n} hopped into the server.</>,
  (n) => <>{n} joined. Say hi!</>,
  (n) => <>Welcome {n}. Say hi!</>,
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
            author: m.author?.global_name || m.author?.username || 'Someone',
            text: m.content || 'Attachment',
          });
      })
      .catch(() => live && setOriginal('gone'));
    return () => {
      live = false;
    };
  }, [channelId, messageId]);
  if (original === 'gone')
    return <span className="lng-muted">The message this thread started from was deleted.</span>;
  return (
    <>
      <span className="lng-muted">Thread started from </span>
      {original ? (
        <>
          <strong className="lng-sys-name">{original.author}</strong>
          <q className="lng-sys-quote">{original.text.slice(0, 200)}</q>
        </>
      ) : (
        <span className="lng-muted">a message…</span>
      )}
      {onJump && (
        <button type="button" className="lng-link-btn" onClick={onJump}>
          {' '}
          Jump to original
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
  const name = <strong className="lng-sys-name">{m.author.displayName}</strong>;
  let icon: IconName = 'arrow-right';
  let body: ReactNode;
  switch (m.type) {
    case SYSTEM_PIN:
      icon = 'pin';
      body = (
        <>
          {name} pinned{' '}
          {referenceId && onJumpTo ? (
            <button type="button" className="lng-link-btn" onClick={() => onJumpTo(referenceId)}>
              a message
            </button>
          ) : (
            'a message'
          )}{' '}
          to this channel.
        </>
      );
      break;
    case SYSTEM_JOIN:
      body = JOIN_LINES[joinLineIndex(m.id)]!(name);
      break;
    case SYSTEM_THREAD_CREATED:
      icon = 'list';
      body = (
        <>
          {name} started a thread:{' '}
          {referenceId && onOpenThread ? (
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => onOpenThread(referenceId)}
            >
              {m.content || 'thread'}
            </button>
          ) : (
            <strong>{m.content || 'a thread'}</strong>
          )}
          .
        </>
      );
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
          <span className="lng-muted">Thread started from a message.</span>
        );
      break;
    case SYSTEM_CALL:
      icon = 'volume';
      body = <>{name} started a call.</>;
      break;
    case SYSTEM_FOLLOW_ADD:
      icon = 'bell';
      body = (
        <>
          {name} has added <strong>{m.content || 'a channel'}</strong> to this channel. Its
          announcements will be posted here.
        </>
      );
      break;
    default:
      if ((SYSTEM_SUPPORT as readonly number[]).includes(m.type ?? -1)) {
        icon = 'star';
        body = <>{name} supported this server.</>;
      } else body = <>{m.content || 'System message'}</>;
  }
  return (
    <article
      className="lng-msg lng-system"
      data-message-id={m.id}
      data-system-type={m.type}
      aria-label={`System message, ${formatMessageTime(m.createdAt)}`}
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
