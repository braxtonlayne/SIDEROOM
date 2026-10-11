import {
  observeElementRect,
  useVirtualizer,
  type Rect,
  type Virtualizer,
} from '@tanstack/react-virtual';
import type { Resolvers } from '@sideroom/markdown-react';
import { Icon } from '@sideroom/ui';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  useChannels,
  useCurrentUser,
  useLoungeActions,
  useMembers,
  useMessages,
  useRoles,
  useUsers,
  type Message,
  type Snowflake,
} from './data';
import { MessageItem } from './MessageItem';
import { buildRows, estimateRow } from './messageRows';
import { useLoungeGoto } from './nav';
import { formatDividerDate } from './time';

/** Reports `fallback` while the scroll element has no layout (jsdom, first paint) so rows still render. */
const FALLBACK: Rect = { width: 600, height: 800 };
const observeRect = <T extends Element>(
  instance: Virtualizer<T, Element>,
  cb: (rect: Rect) => void,
) => observeElementRect(instance, (rect) => cb(rect.height === 0 ? FALLBACK : rect));

const NEAR_BOTTOM = 80;
const NEAR_TOP = 240;

export interface MessageListProps {
  channelId: Snowflake;
  /** Undefined for DMs. */
  guildId?: Snowflake | undefined;
  /** Shown in the welcome header and the accessible name. */
  title: string;
  isDm?: boolean;
  lastReadId?: Snowflake | null | undefined;
  onReply: (m: Message) => void;
  onEdit: (m: Message) => void;
}

export function MessageList({
  channelId,
  guildId,
  title,
  isDm,
  lastReadId,
  onReply,
  onEdit,
}: MessageListProps) {
  const { messages, hasMore, loadingOlder, atLatest } = useMessages(channelId);
  const me = useCurrentUser();
  const users = useUsers();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const actions = useLoungeActions();
  const goto = useLoungeGoto();

  const rows = useMemo(
    () => buildRows(messages, { lastReadId, atStart: !hasMore, selfId: me.id }),
    [messages, lastReadId, hasMore, me.id],
  );

  const byId = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages]);
  const roleColor = useMemo(() => {
    const out = new Map<Snowflake, string>();
    const byRole = new Map(roles.map((r) => [r.id, r]));
    for (const mem of members) {
      const best = mem.roleIds
        .map((id) => byRole.get(id))
        .filter((r): r is NonNullable<typeof r> => !!r && !!r.color)
        .sort((a, b) => b.position - a.position)[0];
      if (best?.color) out.set(mem.user.id, best.color);
    }
    return out;
  }, [members, roles]);

  const resolvers: Resolvers = useMemo(
    () => ({
      user: (id) => {
        const u = users[id];
        return u
          ? { name: u.displayName, ...(roleColor.get(id) ? { color: roleColor.get(id)! } : {}) }
          : undefined;
      },
      role: (id) => {
        const r = roles.find((x) => x.id === id);
        return r ? { name: r.name, ...(r.color ? { color: r.color } : {}) } : undefined;
      },
      channel: (id) => {
        const c = channels.find((x) => x.id === id);
        return c ? { name: c.name } : undefined;
      },
    }),
    [users, roles, channels, roleColor],
  );

  const scroller = useRef<HTMLDivElement | null>(null);
  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => estimateRow(rows[i]!),
    getItemKey: (i) => rows[i]?.key ?? i,
    overscan: 8,
    initialRect: FALLBACK,
    observeElementRect: observeRect,
  });

  const [atBottom, setAtBottom] = useState(true);
  const atBottomRef = useRef(true);
  const lastKey = rows[rows.length - 1]?.key;
  const lastMsg = messages[messages.length - 1];

  const scrollToEnd = useCallback(() => {
    if (rows.length) v.scrollToIndex(rows.length - 1, { align: 'end' });
  }, [v, rows.length]);

  // Opening a channel (or switching) jumps to the newest message.
  const mountedChannel = useRef<Snowflake | null>(null);
  useLayoutEffect(() => {
    if (mountedChannel.current === channelId) return;
    mountedChannel.current = channelId;
    atBottomRef.current = true;
    setAtBottom(true);
    scrollToEnd();
  }, [channelId, scrollToEnd]);

  // New message at the end: follow if the viewer is at the bottom, or it is their own.
  const prevLast = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    if (
      prevLast.current !== undefined &&
      prevLast.current !== lastKey &&
      (atBottomRef.current || lastMsg?.author.id === me.id)
    ) {
      scrollToEnd();
    }
    prevLast.current = lastKey;
  }, [lastKey, lastMsg, me.id, scrollToEnd]);

  // History prepended (Q170): keep the reader's place by the measured growth of the scroll height. The
  // recorded height is taken at the end of every commit, so the next prepend sees the pre-prepend height.
  // scrollToIndex on the previously first message is the fallback when no height was recorded.
  const firstId = messages[0]?.id;
  const prevFirst = useRef<{ channel: Snowflake; id: string | undefined }>({
    channel: channelId,
    id: firstId,
  });
  const prevHeight = useRef(0);
  useLayoutEffect(() => {
    const el = scroller.current;
    const p = prevFirst.current;
    if (p.channel === channelId && p.id && firstId && p.id !== firstId && el) {
      const growth = el.scrollHeight - prevHeight.current;
      if (prevHeight.current > 0 && growth > 0) {
        el.scrollTop += growth;
      } else {
        const idx = rows.findIndex((r) => r.kind === 'message' && r.message.id === p.id);
        if (idx > 0) v.scrollToIndex(idx, { align: 'start' });
      }
    }
    prevFirst.current = { channel: channelId, id: firstId };
    prevHeight.current = el?.scrollHeight ?? 0;
  }, [firstId, channelId, rows, v]);

  // Read state (Q128): the newest message is on screen when the viewer is at the bottom of the live window.
  // Store-side, acks are throttled to one per second per channel.
  const ackId = lastMsg && !lastMsg.pending && (atLatest ?? true) ? lastMsg.id : undefined;
  useEffect(() => {
    if (atBottom && ackId) actions.markRead(channelId);
  }, [actions, atBottom, ackId, channelId]);

  const onScroll = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM;
    atBottomRef.current = near;
    setAtBottom((prev) => (prev === near ? prev : near));
    if (el.scrollTop < NEAR_TOP && hasMore && !loadingOlder) actions.loadOlder(channelId);
  }, [actions, channelId, hasMore, loadingOlder]);

  // Short channels never scroll, so onScroll never asks for more history.
  useEffect(() => {
    const el = scroller.current;
    if (el && el.clientHeight > 0 && el.scrollHeight <= el.clientHeight && hasMore && !loadingOlder)
      actions.loadOlder(channelId);
  }, [actions, channelId, hasMore, loadingOlder, messages.length]);

  const jumpTo = useCallback(
    (id: Snowflake) => {
      const idx = rows.findIndex((r) => r.kind === 'message' && r.message.id === id);
      if (idx >= 0) v.scrollToIndex(idx, { align: 'center' });
    },
    [rows, v],
  );

  const onMention = useCallback(
    (t: { kind: 'user' | 'role' | 'channel'; id: Snowflake }) => {
      if (t.kind === 'channel' && guildId) goto(guildId, t.id);
    },
    [goto, guildId],
  );
  const onReact = useCallback(
    (m: Message, e: string) => actions.toggleReaction(m.channelId, m.id, e),
    [actions],
  );

  return (
    <div className="lng-messages-wrap">
      <div
        ref={scroller}
        className="lng-messages"
        role="log"
        aria-label={`Messages in ${title}`}
        aria-live="polite"
        aria-relevant="additions"
        tabIndex={0}
        onScroll={onScroll}
      >
        <div className="lng-messages-sizer" style={{ height: v.getTotalSize() }}>
          {v.getVirtualItems().map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            return (
              <div
                key={item.key}
                ref={v.measureElement}
                data-index={item.index}
                className="lng-row"
                style={{ transform: `translateY(${item.start}px)` }}
              >
                {row.kind === 'welcome' && (
                  <div className="lng-welcome">
                    <h2>{isDm ? title : `Welcome to #${title}!`}</h2>
                    <p>
                      {isDm
                        ? `This is the beginning of your direct message history with ${title}.`
                        : `This is the start of the #${title} channel.`}
                    </p>
                  </div>
                )}
                {row.kind === 'date' && (
                  <div
                    className="lng-divider"
                    role="separator"
                    aria-label={formatDividerDate(row.iso)}
                  >
                    <span>{formatDividerDate(row.iso)}</span>
                  </div>
                )}
                {row.kind === 'new' && (
                  <div
                    className="lng-divider lng-divider-new"
                    role="separator"
                    aria-label="New messages"
                  >
                    <span>NEW</span>
                  </div>
                )}
                {row.kind === 'message' && (
                  <MessageItem
                    message={row.message}
                    grouped={row.grouped}
                    replyTarget={
                      row.message.replyToId ? byId.get(row.message.replyToId) : undefined
                    }
                    nameColor={roleColor.get(row.message.author.id)}
                    resolvers={resolvers}
                    isOwn={row.message.author.id === me.id}
                    onReply={onReply}
                    onEdit={onEdit}
                    onReact={onReact}
                    onMentionClick={onMention}
                    onJumpTo={jumpTo}
                  />
                )}
              </div>
            );
          })}
        </div>
        {loadingOlder && (
          <div className="lng-loading-older" role="status">
            Loading earlier messages…
          </div>
        )}
      </div>
      {!atBottom && (
        <button type="button" className="lng-jump" onClick={scrollToEnd}>
          You&apos;re viewing older messages
          <span>
            Jump To Present <Icon name="chevron-down" size={16} />
          </span>
        </button>
      )}
    </div>
  );
}
