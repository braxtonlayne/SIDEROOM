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
  useGuild,
  useLoungeActions,
  useMembers,
  useMessages,
  useRoles,
  useUsers,
  type Message,
  type Snowflake,
} from './data';
import { MessageItem, type ReplyState } from './MessageItem';
import { DeleteMessageDialog, ForwardDialog, PinMessageDialog } from './msg/MessageDialogs';
import { ReactorsModal } from './msg/Reactors';
import { cmpId } from '../store/reducer';
import { buildRows, estimateRow } from './messageRows';
import { useLoungeGoto, useLoungeRoute } from './nav';
import { formatDividerDate } from './time';
import './msg/msg.css';

/** Reports `fallback` while the scroll element has no layout (jsdom, first paint) so rows still render. */
const FALLBACK: Rect = { width: 600, height: 800 };
const observeRect = <T extends Element>(
  instance: Virtualizer<T, Element>,
  cb: (rect: Rect) => void,
) => observeElementRect(instance, (rect) => cb(rect.height === 0 ? FALLBACK : rect));

const NEAR_BOTTOM = 80;
/** Scroll offset of channels the viewer left while reading history (restored when they come back). */
const savedScroll = new Map<string, number>();
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
  /** Message being edited in place, if any. */
  editingId?: Snowflake | null | undefined;
  onCancelEdit?: (() => void) | undefined;
}

export function MessageList({
  channelId,
  guildId,
  title,
  isDm,
  lastReadId,
  onReply,
  onEdit,
  editingId,
  onCancelEdit,
}: MessageListProps) {
  const { messages, hasMore, loadingOlder, atLatest } = useMessages(channelId);
  const me = useCurrentUser();
  const users = useUsers();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const actions = useLoungeActions();
  const goto = useLoungeGoto();
  const guild = useGuild(guildId);
  // Channel-level bits (overwrites applied) win over the guild-level fallback (D-178).
  const chanPerms = channels.find((c) => c.id === channelId)?.permissions;
  const canManage = chanPerms ? chanPerms.manageMessages : !!guild?.permissions.manageMessages;
  const canPin = chanPerms ? chanPerms.pinMessages : guild ? !!guild.permissions.pinMessages : true;
  const [deleting, setDeleting] = useState<Message | null>(null);
  const [pinning, setPinning] = useState<Message | null>(null);
  const [forwarding, setForwarding] = useState<Message | null>(null);
  const [reactors, setReactors] = useState<{ m: Message; emoji?: string } | null>(null);
  const myRoleIds = useMemo(
    () => members.find((mem) => mem.user.id === me.id)?.roleIds ?? [],
    [members, me.id],
  );
  const { messageId: targetId } = useLoungeRoute();

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

  const [bannerDismissed, setBannerDismissed] = useState(false);
  const unreadCount = useMemo(() => {
    const idx = lastReadId ? messages.findIndex((m) => m.id === lastReadId) : -1;
    return idx < 0 ? 0 : messages.slice(idx + 1).filter((m) => m.author.id !== me.id).length;
  }, [messages, lastReadId, me.id]);
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
    const saved = savedScroll.get(channelId);
    if (saved !== undefined && !targetId) {
      atBottomRef.current = false;
      setAtBottom(false);
      v.scrollToOffset(saved);
      return;
    }
    atBottomRef.current = true;
    setAtBottom(true);
    scrollToEnd();
  }, [channelId, scrollToEnd, v, targetId]);
  useEffect(() => {
    const el = scroller.current;
    return () => {
      if (!el) return;
      const away = el.scrollHeight - el.scrollTop - el.clientHeight >= NEAR_BOTTOM;
      if (away) savedScroll.set(channelId, el.scrollTop);
      else savedScroll.delete(channelId);
    };
  }, [channelId]);

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

  const flash = useCallback((id: Snowflake) => {
    setTimeout(() => {
      const el = scroller.current?.querySelector(`[data-message-id="${id}"]`);
      el?.setAttribute('data-flash', '');
      setTimeout(() => el?.removeAttribute('data-flash'), 2000);
    }, 120);
  }, []);

  // A reply preview or pin notice pointing outside the loaded window: load around it, then scroll.
  const [wanted, setWanted] = useState<Snowflake | null>(null);
  const jumpTo = useCallback(
    (id: Snowflake) => {
      const idx = rows.findIndex((r) => r.kind === 'message' && r.message.id === id);
      if (idx >= 0) {
        v.scrollToIndex(idx, { align: 'center' });
        flash(id);
      } else if (actions.revealMessage) {
        setWanted(id);
        actions.revealMessage(channelId, id);
      }
    },
    [rows, v, flash, actions, channelId],
  );
  useEffect(() => {
    if (!wanted || !byId.has(wanted)) return;
    const idx = rows.findIndex((r) => r.kind === 'message' && r.message.id === wanted);
    if (idx < 0) return;
    setWanted(null);
    v.scrollToIndex(idx, { align: 'center' });
    flash(wanted);
  }, [wanted, byId, rows, v, flash]);

  // /channels/:guild/:channel/:message (search "jump"): load the page around it if needed, scroll to it
  // once and flash it.
  const jumped = useRef<string | null>(null);
  const loadedCount = messages.length;
  const hasTarget = !!targetId && byId.has(targetId);
  const revealed = useRef<string | null>(null);
  useEffect(() => {
    if (!targetId || hasTarget || loadedCount === 0 || loadingOlder) return;
    if (revealed.current === `${channelId}:${targetId}`) return; // once; a deleted message stays absent
    revealed.current = `${channelId}:${targetId}`;
    actions.revealMessage?.(channelId, targetId);
  }, [actions, channelId, targetId, hasTarget, loadedCount, loadingOlder]);
  useEffect(() => {
    if (!targetId || !hasTarget || jumped.current === `${channelId}:${targetId}`) return;
    jumped.current = `${channelId}:${targetId}`;
    const idx = rows.findIndex((r) => r.kind === 'message' && r.message.id === targetId);
    if (idx < 0) return;
    v.scrollToIndex(idx, { align: 'center' });
    flash(targetId);
  }, [channelId, targetId, hasTarget, rows, v, flash]);

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
  const onDelete = useCallback(
    (m: Message, skipConfirm: boolean) => {
      if (skipConfirm) actions.deleteMessage?.(m.channelId, m.id);
      else setDeleting(m);
    },
    [actions],
  );
  const onPin = useCallback((m: Message) => setPinning(m), []);
  const onForward = useCallback((m: Message) => setForwarding(m), []);
  const onViewReactions = useCallback((m: Message, emoji?: string) => {
    setReactors(emoji ? { m, emoji } : { m });
  }, []);
  const onSaveEdit = useCallback(
    (m: Message, content: string) => {
      actions.editMessage(m.channelId, m.id, content);
      onCancelEdit?.();
    },
    [actions, onCancelEdit],
  );
  const onVote = useCallback(
    (m: Message, answerId: number, add: boolean) =>
      actions.votePoll?.(m.channelId, m.id, answerId, add),
    [actions],
  );
  const onEndPoll = useCallback((m: Message) => actions.endPoll?.(m.channelId, m.id), [actions]);
  const onRetry = useCallback((m: Message) => actions.retryMessage?.(m.channelId, m.id), [actions]);
  const onDiscard = useCallback(
    (m: Message) => actions.discardMessage?.(m.channelId, m.id),
    [actions],
  );

  const oldestId = messages[0]?.id;
  const replyState = (m: Message): ReplyState => {
    if (!m.replyToId) return 'loaded';
    if (byId.has(m.replyToId)) return 'loaded';
    // Inside the loaded range but absent: the original was deleted.
    return oldestId && cmpId(m.replyToId, oldestId) > 0 ? 'deleted' : 'unloaded';
  };
  const mentionsMe = (m: Message) =>
    m.author.id !== me.id &&
    (!!m.mentionIds?.includes(me.id) ||
      !!m.mentionEveryone ||
      !!m.mentionRoleIds?.some((r) => myRoleIds.includes(r)));

  return (
    <div className="lng-messages-wrap">
      {unreadCount > 0 && !bannerDismissed && !atBottom && (
        <div className="lng-unread-banner" role="status">
          <span>
            {unreadCount >= 50 ? '50+' : unreadCount} new message{unreadCount === 1 ? '' : 's'}
          </span>
          <button
            type="button"
            onClick={() => {
              actions.markRead(channelId);
              setBannerDismissed(true);
            }}
          >
            Mark As Read
          </button>
        </div>
      )}
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
                    replyState={replyState(row.message)}
                    guildId={guildId}
                    mentionsMe={mentionsMe(row.message)}
                    canManage={canManage}
                    canPin={canPin}
                    editing={editingId === row.message.id}
                    onDelete={actions.deleteMessage ? onDelete : undefined}
                    onPin={actions.pinMessage ? onPin : undefined}
                    onForward={actions.forwardMessage ? onForward : undefined}
                    onViewReactions={onViewReactions}
                    onSaveEdit={onSaveEdit}
                    onCancelEdit={onCancelEdit}
                    onVote={onVote}
                    onEndPoll={actions.endPoll ? onEndPoll : undefined}
                    onRetry={onRetry}
                    onDiscard={onDiscard}
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
      {(!atBottom || atLatest === false) && (
        <button
          type="button"
          className="lng-jump"
          onClick={() => {
            if (atLatest === false && actions.jumpToPresent) {
              jumped.current = null;
              actions.jumpToPresent(channelId);
              goto(guildId ?? '@me', channelId);
            } else scrollToEnd();
          }}
        >
          You&apos;re viewing older messages
          <span>
            Jump To Present <Icon name="chevron-down" size={16} />
          </span>
        </button>
      )}
      <DeleteMessageDialog
        message={deleting}
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) actions.deleteMessage?.(deleting.channelId, deleting.id);
          if (deleting?.id === editingId) onCancelEdit?.();
          setDeleting(null);
        }}
      />
      <PinMessageDialog
        message={pinning}
        pinned={!!pinning?.pinned}
        onCancel={() => setPinning(null)}
        onConfirm={() => {
          if (pinning) actions.pinMessage?.(pinning.channelId, pinning.id, !pinning.pinned);
          setPinning(null);
        }}
      />
      <ForwardDialog
        message={forwarding}
        onClose={() => setForwarding(null)}
        onForward={(targets, note) => {
          if (forwarding)
            actions.forwardMessage?.(
              {
                channelId: forwarding.channelId,
                messageId: forwarding.id,
                ...(guildId ? { guildId } : {}),
              },
              targets,
              note,
            );
        }}
      />
      <ReactorsModal
        message={reactors ? (byId.get(reactors.m.id) ?? reactors.m) : null}
        initialEmoji={reactors?.emoji}
        canManage={canManage}
        onClose={() => setReactors(null)}
      />
    </div>
  );
}
