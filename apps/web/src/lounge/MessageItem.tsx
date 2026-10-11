import { Markdown, type Resolvers } from '@sideroom/markdown-react';
import { Avatar, DropdownMenu, Icon, Popover, Tooltip, type MenuEntry } from '@sideroom/ui';
import { memo } from 'react';
import { EmojiPicker } from './components/EmojiPicker';
import type { Message, Snowflake } from './data';
import { isJumbo, parseMarkdown } from './markdown';
import { formatClock, formatFullTime, formatMessageTime } from './time';

export interface MessageItemProps {
  message: Message;
  grouped: boolean;
  /** Message this one replies to, if loaded. */
  replyTarget?: Message | undefined;
  /** CSS color of the author's highest colored role. */
  nameColor?: string | undefined;
  resolvers: Resolvers;
  isOwn: boolean;
  onReply: (m: Message) => void;
  onEdit: (m: Message) => void;
  onReact: (m: Message, emoji: string) => void;
  onMentionClick?: (t: { kind: 'user' | 'role' | 'channel'; id: Snowflake }) => void;
  onJumpTo?: (id: Snowflake) => void;
}

const QUICK = ['👍', '😂', '🎉'];

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function MessageBody({
  m,
  resolvers,
  onMentionClick,
}: Pick<MessageItemProps, 'resolvers' | 'onMentionClick'> & { m: Message }) {
  const nodes = parseMarkdown(m.content);
  return (
    <>
      {m.content && (
        <Markdown
          nodes={nodes}
          jumbo={isJumbo(nodes)}
          resolvers={resolvers}
          {...(onMentionClick ? { onMentionClick } : {})}
        />
      )}
      {m.editedAt && (
        <span className="lng-edited" title={formatFullTime(m.editedAt)}>
          {' '}
          (edited)
        </span>
      )}
      {m.attachments?.map((a) =>
        a.contentType.startsWith('image/') ? (
          <img
            key={a.id}
            className="lng-attach-img"
            src={a.url}
            alt={a.filename}
            width={a.width}
            height={a.height}
            loading="lazy"
          />
        ) : (
          <a key={a.id} className="lng-attach-file" href={a.url} download={a.filename}>
            <Icon name="download" size={20} />
            <span>{a.filename}</span>
            <span className="lng-muted">{formatSize(a.size)}</span>
          </a>
        ),
      )}
    </>
  );
}

function MessageItemImpl(props: MessageItemProps) {
  const {
    message: m,
    grouped,
    replyTarget,
    nameColor,
    isOwn,
    onReply,
    onEdit,
    onReact,
    onJumpTo,
  } = props;
  const moreItems: MenuEntry[] = [
    { label: 'Copy Text', onSelect: () => void navigator.clipboard?.writeText(m.content) },
    {
      label: 'Copy Message Link',
      onSelect: () =>
        void navigator.clipboard?.writeText(`${location.origin}${location.pathname}/${m.id}`),
    },
    { label: 'Copy Message ID', onSelect: () => void navigator.clipboard?.writeText(m.id) },
  ];
  const toolbar = (
    <div
      className="lng-msg-tools"
      role="toolbar"
      aria-label={`Actions for message by ${m.author.displayName}`}
    >
      {QUICK.slice(0, 2).map((e) => (
        <button
          key={e}
          type="button"
          className="lng-icon-btn"
          aria-label={`React with ${e}`}
          onClick={() => onReact(m, e)}
        >
          <span aria-hidden="true">{e}</span>
        </button>
      ))}
      <Popover
        label="Add reaction"
        placement="left"
        trigger={
          <button type="button" className="lng-icon-btn" aria-label="Add Reaction">
            <Icon name="smile" size={20} />
          </button>
        }
      >
        <EmojiPicker onPick={(e) => onReact(m, e)} />
      </Popover>
      <Tooltip content="Reply">
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Reply"
          onClick={() => onReply(m)}
        >
          <Icon name="reply" size={20} />
        </button>
      </Tooltip>
      {isOwn && (
        <Tooltip content="Edit">
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Edit"
            onClick={() => onEdit(m)}
          >
            <Icon name="edit" size={20} />
          </button>
        </Tooltip>
      )}
      <DropdownMenu
        label="More"
        items={moreItems}
        placement="bottom-end"
        trigger={
          <button type="button" className="lng-icon-btn" aria-label="More">
            <Icon name="more-horizontal" size={20} />
          </button>
        }
      />
    </div>
  );

  return (
    <article
      className="lng-msg"
      data-grouped={grouped || undefined}
      data-pending={m.pending || undefined}
      data-failed={m.failed || undefined}
      data-message-id={m.id}
      aria-label={`${m.author.displayName}, ${formatMessageTime(m.createdAt)}`}
    >
      {m.replyToId && (
        <button
          type="button"
          className="lng-reply"
          onClick={() => onJumpTo?.(m.replyToId!)}
          disabled={!replyTarget}
        >
          <Icon name="reply" size={16} />
          {replyTarget ? (
            <>
              <span className="lng-reply-name">@{replyTarget.author.displayName}</span>
              <span className="lng-reply-text">
                {replyTarget.content.slice(0, 120) || 'Click to see attachment'}
              </span>
            </>
          ) : (
            <span className="lng-reply-text lng-muted">Original message was not loaded</span>
          )}
        </button>
      )}
      <div className="lng-msg-main">
        <div className="lng-msg-gutter">
          {grouped ? (
            <time
              className="lng-msg-hover-time"
              dateTime={m.createdAt}
              title={formatFullTime(m.createdAt)}
            >
              {formatClock(new Date(m.createdAt))}
            </time>
          ) : (
            <Avatar name={m.author.displayName} src={m.author.avatarUrl ?? null} size={40} />
          )}
        </div>
        <div className="lng-msg-body">
          {!grouped && (
            <header className="lng-msg-head">
              <span className="lng-msg-author" style={nameColor ? { color: nameColor } : undefined}>
                {m.author.displayName}
              </span>
              {m.author.tag && <span className="lng-tag">{m.author.tag}</span>}
              {m.author.bot && <span className="lng-bot">APP</span>}
              <time
                className="lng-msg-time"
                dateTime={m.createdAt}
                title={formatFullTime(m.createdAt)}
              >
                {formatMessageTime(m.createdAt)}
              </time>
            </header>
          )}
          <div className="lng-msg-content">
            <MessageBody
              m={m}
              resolvers={props.resolvers}
              {...(props.onMentionClick ? { onMentionClick: props.onMentionClick } : {})}
            />
          </div>
          {m.failed && <p className="lng-failed">Your message could not be delivered.</p>}
          {m.reactions && m.reactions.length > 0 && (
            <div className="lng-reactions">
              {m.reactions.map((r) => (
                <button
                  key={r.emoji}
                  type="button"
                  className="lng-reaction"
                  aria-pressed={r.me}
                  aria-label={`${r.emoji} ${r.count} ${r.count === 1 ? 'reaction' : 'reactions'}${r.me ? ', including you' : ''}`}
                  onClick={() => onReact(m, r.emoji)}
                >
                  <span aria-hidden="true">{r.emoji}</span>{' '}
                  <span aria-hidden="true">{r.count}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
      {toolbar}
    </article>
  );
}

export const MessageItem = memo(MessageItemImpl);
