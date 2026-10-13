import { Icon, Popover, Tooltip } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { apiFetch } from '../../api/client';
import { EmojiPicker } from '../components/EmojiPicker';
import type { Message, Reaction } from '../data';

interface Reactor {
  id: string;
  username: string;
  global_name?: string | null;
}

/** Reactor names fetched for a tooltip, keyed by message, emoji and count so a changed count refetches. */
const nameCache = new Map<string, string[]>();
export const resetReactorNames = () => nameCache.clear();

/** "Ann, Bob and 3 others reacted with X" (LNG-MSGUI-023). Mounted only while the tooltip is open. */
export function reactorSentence(names: string[], total: number, emoji: string): string {
  if (total <= 0) return '';
  const shown = names.slice(0, 3);
  const rest = total - shown.length;
  let who: string;
  if (shown.length === 0) who = `${total} ${total === 1 ? 'person' : 'people'}`;
  else if (rest > 0) who = `${shown.join(', ')} and ${rest} ${rest === 1 ? 'other' : 'others'}`;
  else if (shown.length === 1) who = shown[0]!;
  else who = `${shown.slice(0, -1).join(', ')} and ${shown[shown.length - 1]!}`;
  return `${who} reacted with ${emoji}`;
}

function ReactorNames({ message, reaction }: { message: Message; reaction: Reaction }) {
  const key = `${message.id}:${reaction.emoji}:${reaction.count}`;
  const [names, setNames] = useState<string[] | null>(nameCache.get(key) ?? null);
  useEffect(() => {
    if (names) return;
    let live = true;
    apiFetch<Reactor[]>(
      `/channels/${message.channelId}/messages/${message.id}/reactions/${encodeURIComponent(reaction.emoji)}?limit=4`,
      { retries: 0 },
    )
      .then((list) => {
        const n = list.map((u) => u.global_name || u.username);
        nameCache.set(key, n);
        if (live) setNames(n);
      })
      .catch(() => live && setNames([]));
    return () => {
      live = false;
    };
  }, [key, message.channelId, message.id, reaction.emoji, names]);
  return <span>{reactorSentence(names ?? [], reaction.count, reaction.emoji)}</span>;
}

/** One reaction pill. A burst (pop and ring) plays when the viewer's own reaction is added. */
function ReactionPill({
  message,
  reaction,
  onReact,
  onViewReactions,
}: {
  message: Message;
  reaction: Reaction;
  onReact: (m: Message, emoji: string) => void;
  onViewReactions?: ((m: Message, emoji?: string) => void) | undefined;
}) {
  const prevMe = useRef(reaction.me);
  const [burst, setBurst] = useState(false);
  useEffect(() => {
    if (reaction.me && !prevMe.current) {
      setBurst(true);
      const t = setTimeout(() => setBurst(false), 700);
      prevMe.current = reaction.me;
      return () => clearTimeout(t);
    }
    prevMe.current = reaction.me;
    return undefined;
  }, [reaction.me]);
  return (
    <Tooltip content={<ReactorNames message={message} reaction={reaction} />} delay={300}>
      <button
        type="button"
        className="lng-reaction"
        aria-pressed={reaction.me}
        data-burst={burst || undefined}
        aria-label={`${reaction.emoji} ${reaction.count} ${reaction.count === 1 ? 'reaction' : 'reactions'}${reaction.me ? ', including you' : ''}`}
        onClick={() => onReact(message, reaction.emoji)}
        onContextMenu={(e) => {
          if (!onViewReactions) return;
          e.preventDefault();
          e.stopPropagation();
          onViewReactions(message, reaction.emoji);
        }}
      >
        <span aria-hidden="true" className="lng-reaction-emoji">
          {reaction.emoji}
        </span>{' '}
        <span aria-hidden="true">{reaction.count}</span>
      </button>
    </Tooltip>
  );
}

/** The reactions under a message, with an inline add-reaction button at the end of the row. */
export function ReactionRow({
  message,
  onReact,
  onViewReactions,
}: {
  message: Message;
  onReact: (m: Message, emoji: string) => void;
  onViewReactions?: ((m: Message, emoji?: string) => void) | undefined;
}) {
  const list = message.reactions ?? [];
  if (list.length === 0) return null;
  return (
    <div className="lng-reactions">
      {list.map((r) => (
        <ReactionPill
          key={r.emoji}
          message={message}
          reaction={r}
          onReact={onReact}
          onViewReactions={onViewReactions}
        />
      ))}
      {!message.pending && !message.failed && (
        <Popover
          label="Add reaction"
          placement="top"
          trigger={
            <button
              type="button"
              className="lng-reaction lng-reaction-add"
              aria-label="Add reaction to this message"
            >
              <Icon name="smile" size={16} />
            </button>
          }
        >
          <EmojiPicker onPick={(e) => onReact(message, e)} />
        </Popover>
      )}
    </div>
  );
}
