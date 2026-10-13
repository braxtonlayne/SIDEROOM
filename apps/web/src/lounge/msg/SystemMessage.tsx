import { Icon } from '@sideroom/ui';
import type { Message, Snowflake } from '../data';
import { formatMessageTime } from '../time';

/** Discord message types that are not authored chat (LNG-MSGUI-030). */
export const SYSTEM_PIN = 6;
export const SYSTEM_JOIN = 7;

export const isSystemMessage = (m: Pick<Message, 'type'>) =>
  m.type !== undefined && m.type !== 0 && m.type !== 19 && m.type !== 20;

/** Row for pin notices and member joins. Other system types render a neutral line. */
export function SystemMessage({
  message: m,
  referenceId,
  onJumpTo,
}: {
  message: Message;
  referenceId?: Snowflake | null;
  onJumpTo?: ((id: Snowflake) => void) | undefined;
}) {
  const name = <strong className="lng-sys-name">{m.author.displayName}</strong>;
  let body;
  switch (m.type) {
    case SYSTEM_PIN:
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
      body = <>{name} joined the party.</>;
      break;
    default:
      body = <>{m.content || 'System message'}</>;
  }
  return (
    <article
      className="lng-msg lng-system"
      data-message-id={m.id}
      aria-label={`System message, ${formatMessageTime(m.createdAt)}`}
    >
      <div className="lng-msg-main">
        <div className="lng-msg-gutter">
          <Icon name={m.type === SYSTEM_PIN ? 'pin' : 'arrow-right'} size={20} />
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
