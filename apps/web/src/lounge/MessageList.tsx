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
import { announce } from '../a11y/announcer';
import { useT } from '../i18n';
import { MessageItem, type NameDecor, type ReplyState } from './MessageItem';
import { DeleteMessageDialog, ForwardDialog, PinMessageDialog } from './msg/MessageDialogs';
import { ReactorsModal } from './msg/Reactors';
import { cmpId } from '../store/reducer';
import { buildRows, estimateRow } from './messageRows';
import { openProfilePopout } from './profile/popoutStore';
import { DmWelcome } from './shell/DmWelcome';
import { useHiddenAuthors } from './shell/hiddenAuthors';
import { toSaved, useSaved } from './shell/savedMessages';
import { useLoungeUi } from './store';
import { useLoungeGoto, useLoungeGotoMessage, useLoungeGotoThread, useLoungeRoute } from './nav';
import { formatDividerDate } from './time';
import { customEmojiResolver } from './msg/CustomEmoji';
import { richCardResolvers } from './msg/RichCards';
import './msg/msg.css';

/** Reports `fallback` while the scroll element has no layout (jsdom, first paint) so rows still render. */
const FALLBACK: Rect = { width: 600, height: 800 };
const observeRect = <T extends Element>(
  instance: Virtualizer<T, Element>,
  cb: (rect: Rect) => void,
) => observeElementRect(instance, (rect) => cb(rect.height === 0 ? FALLBACK : rect));

/** Reads text aloud with the browser's speech synthesis, when it has one. */
function speak(text: string) {
  if (typeof speechSynthesis === 'undefined') return;
  speechSynthesis.speak(new SpeechSynthesisUtterance(text));
}

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
  /** This list is a thread: no nested threads. */
  inThread?: boolean | undefined;
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
  inThread,
}: MessageListProps) {
  const t = useT();
  const { messages, hasMore, loadingOlder, atLatest } = useMessages(channelId);
  const me = useCurrentUser();
  const users = useUsers();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const actions = useLoungeActions();
  const goto = useLoungeGoto();
  const gotoThread = useLoungeGotoThread();
  const gotoMessage = useLoungeGotoMessage();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const bookmarks = useSaved((s) => s.bookmarks);
  const guild = useGuild(guildId);
  // Channel-level bits (overwrites applied) win over the guild-level fallback (D-178).
  const chanPerms = channels.find((c) => c.id === channelId)?.permissions;
  const canManage = chanPerms ? chanPerms.manageMessages : !!guild?.permissions.manageMessages;
  const canPin = chanPerms ? chanPerms.pinMessages : guild ? !!guild.permissions.pinMessages : true;
  const canEditChannel =
    !!guildId && !inThread && (chanPerms?.manageChannels ?? !!guild?.permissions.manageChannels);
  const [deleting, setDeleting] = useState<Message | null>(null);
  const [pinning, setPinning] = useState<Message | null>(null);
  const [forwarding, setForwarding] = useState<Message | null>(null);
  const [reactors, setReactors] = useState<{ m: Message; emoji?: string } | null>(null);
  const myRoleIds = useMemo(
    () => members.find((mem) => mem.user.id === me.id)?.roleIds ?? [],
    [members, me.id],
  );
  const { messageId: targetId } = useLoungeRoute();

  const canThread =
    !!guildId && !inThread && (chanPerms ? chanPerms.createThreads !== false : true);
  // Mark Unread moves the NEW divider before the chosen message without leaving the channel.
  const [unreadFrom, setUnreadFrom] = useState<Snowflake | null>(null);
  const hidden = useHiddenAuthors();
  const [shownRuns, setShownRuns] = useState<ReadonlySet<string>>(() => new Set());
  const rows = useMemo(
    () =>
      buildRows(messages, {
        lastReadId: unreadFrom ?? lastReadId,
        atStart: !hasMore,
        // Mark Unread is the viewer's own choice, so its divider shows even above their own message.
        ...(unreadFrom ? {} : { selfId: me.id }),
        hidden,
        revealed: shownRuns,
      }),
    [messages, lastReadId, unreadFrom, hasMore, me.id, hidden, shownRuns],
  );

  const byId = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages]);
  // Name colour of each member's highest coloured role, plus that role's gradient and icon (LNG-MSGUI-002).
  const roleDecor = useMemo(() => {
    const out = new Map<Snowflake, NameDecor>();
    const byRole = new Map(roles.map((r) => [r.id, r]));
    for (const mem of members) {
      const best = mem.roleIds
        .map((id) => byRole.get(id))
        .filter((r): r is NonNullable<typeof r> => !!r && !!r.color)
        .sort((a, b) => b.position - a.position)[0];
      const iconRole = mem.roleIds
        .map((id) => byRole.get(id))
        .filter((r): r is NonNullable<typeof r> => !!r && !!r.emoji)
        .sort((a, b) => b.position - a.position)[0];
      if (best?.color || iconRole)
        out.set(mem.user.id, {
          ...(best?.color ? { color: best.color } : {}),
          ...(best?.colorSecondary ? { secondary: best.colorSecondary } : {}),
          ...(iconRole?.emoji ? { icon: iconRole.emoji, iconName: iconRole.name } : {}),
        });
    }
    return out;
  }, [members, roles]);
  const roleColor = useMemo(() => {
    const out = new Map<Snowflake, string>();
    for (const [id, d] of roleDecor) if (d.color) out.set(id, d.color);
    return out;
  }, [roleDecor]);

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
      customEmoji: customEmojiResolver(guildId),
      ...richCardResolvers,
    }),
    [users, roles, channels, roleColor, guildId],
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
    const from = unreadFrom ?? lastReadId;
    const idx = from ? messages.findIndex((m) => m.id === from) : -1;
    return idx < 0 ? 0 : messages.slice(idx + 1).filter((m) => m.author.id !== me.id).length;
  }, [messages, lastReadId, unreadFrom, me.id]);
  // First unread message and first unread mention for the floating jump pills (LNG-CHN-012).
  const { firstUnreadId, mentionIds } = useMemo(() => {
    const from = unreadFrom ?? lastReadId;
    const idx = from ? messages.findIndex((m) => m.id === from) : -1;
    const after = idx < 0 ? [] : messages.slice(idx + 1).filter((m) => m.author.id !== me.id);
    const ids = after
      .filter(
        (m) =>
          !!m.mentionIds?.includes(me.id) ||
          !!m.mentionEveryone ||
          !!m.mentionRoleIds?.some((r) => myRoleIds.includes(r)),
      )
      .map((m) => m.id);
    return { firstUnreadId: after[0]?.id, mentionIds: ids };
  }, [messages, lastReadId, unreadFrom, me.id, myRoleIds]);
  const [atBottom, setAtBottom] = useState(true);
  const atBottomRef = useRef(true);
  const lastKey = rows[rows.length - 1]?.key;
  const lastMsg = messages[messages.length - 1];
  // A jumped-to window is not at the newest message: appended pages must not drag the view to the end.
  const wasLatest = useRef(atLatest ?? true);

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
      wasLatest.current &&
      (atBottomRef.current || lastMsg?.author.id === me.id)
    ) {
      scrollToEnd();
    }
    prevLast.current = lastKey;
    wasLatest.current = atLatest ?? true;
  }, [lastKey, lastMsg, me.id, scrollToEnd, atLatest]);

  // Screen readers hear a new message from someone else in the open channel (XC-A11Y-002) without focus
  // moving. Loading history or opening a channel changes the last row too, so only a newly arrived id counts.
  const announced = useRef<{ channel: Snowflake; id: string | undefined } | null>(null);
  useEffect(() => {
    const prev = announced.current;
    announced.current = { channel: channelId, id: lastMsg?.id };
    if (!prev || prev.channel !== channelId || !lastMsg || prev.id === lastMsg.id) return;
    if (lastMsg.pending || lastMsg.author.id === me.id) return;
    if (!prev.id || cmpId(lastMsg.id, prev.id) <= 0) return;
    const author = lastMsg.author.displayName;
    const text = lastMsg.content.trim();
    announce(
      text
        ? t('a11y.announce.messageFrom', { author, text: text.slice(0, 200) })
        : t('a11y.announce.messageFromAttachment', { author }),
    );
  }, [channelId, lastMsg, me.id, t]);

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
    // Viewing a jumped-to window: reaching the bottom loads the next page (LNG-MSGUI-001).
    if (near && atLatest === false && !loadingOlder) actions.loadNewer?.(channelId);
  }, [actions, channelId, hasMore, loadingOlder, atLatest]);

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
      // The mention that was just clicked still has focus: the popout opens next to it.
      else if (t.kind === 'user')
        openProfilePopout(
          { userId: t.id, guildId },
          document.activeElement instanceof HTMLElement ? document.activeElement : null,
        );
    },
    [goto, guildId],
  );
  const onOpenProfile = useCallback(
    (userId: Snowflake, anchor: HTMLElement) => openProfilePopout({ userId, guildId }, anchor),
    [guildId],
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

  const onMarkUnread = useCallback(
    (m: Message) => {
      void actions.markUnread?.(m.channelId, m.id).then((before) => {
        setUnreadFrom(before);
        setBannerDismissed(false);
      });
    },
    [actions],
  );
  const onBookmark = useCallback(
    (m: Message) => void useSaved.getState().toggleBookmark(toSaved(m, guildId)),
    [guildId],
  );
  const onRemind = useCallback(
    (m: Message, ms: number) =>
      void useSaved
        .getState()
        .addReminder({ note: '', remindAt: new Date(Date.now() + ms), ref: toSaved(m, guildId) })
        .catch(() => undefined),
    [guildId],
  );
  const onCreateThread = useCallback(
    (m: Message) => {
      if (guildId)
        openDialog({ kind: 'createThread', guildId, channelId: m.channelId, messageId: m.id });
    },
    [guildId, openDialog],
  );
  const onOpenThread = useCallback(
    (m: Message, threadId: Snowflake) => {
      if (guildId) gotoThread(guildId, m.channelId, threadId);
    },
    [guildId, gotoThread],
  );
  // Leaving a channel lets automatic read acks resume (Mark Unread holds them while it is open).
  useEffect(
    () => () => {
      actions.releaseUnread?.(channelId);
    },
    [actions, channelId],
  );
  // /tts messages from other people are read aloud as they arrive in the open channel.
  const spoken = useRef<string | null>(lastMsg?.id ?? null);
  useEffect(() => {
    if (!lastMsg || spoken.current === lastMsg.id) return;
    spoken.current = lastMsg.id;
    if (lastMsg.tts && lastMsg.author.id !== me.id && !lastMsg.pending)
      speak(
        t('lounge.messageList.speechText', {
          name: lastMsg.author.displayName,
          content: lastMsg.content,
        }),
      );
  }, [lastMsg, me.id, t]);

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
          <button
            type="button"
            className="lng-unread-jump"
            onClick={() => firstUnreadId && jumpTo(firstUnreadId)}
          >
            {t('lounge.messageList.newMessages', {
              count: unreadCount >= 50 ? '50+' : unreadCount,
              n: unreadCount,
            })}
          </button>
          <button
            type="button"
            onClick={() => {
              actions.releaseUnread?.(channelId);
              setUnreadFrom(null);
              actions.markRead(channelId);
              setBannerDismissed(true);
            }}
          >
            {t('lounge.messageList.markAsRead')}
          </button>
        </div>
      )}
      {mentionIds.length > 0 && !bannerDismissed && (
        <button
          type="button"
          className="lng-jump-pill"
          data-edge="top"
          style={{ insetBlockStart: unreadCount > 0 && !atBottom ? 40 : 8 }}
          onClick={() => mentionIds[0] && jumpTo(mentionIds[0])}
        >
          {t('lounge.jumpPills.mentions', { n: mentionIds.length })}
        </button>
      )}
      <div
        ref={scroller}
        className="lng-messages"
        role="log"
        aria-label={t('lounge.messageList.messagesIn', { title })}
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
                {row.kind === 'welcome' && isDm && (
                  <DmWelcome channelId={channelId} title={title} />
                )}
                {row.kind === 'welcome' && !isDm && (
                  <div className="lng-welcome">
                    <h2>{t('lounge.messageList.welcomeChannel', { title })}</h2>
                    <p>{t('lounge.messageList.channelStart', { title })}</p>
                    {canEditChannel && guildId && (
                      <div className="lng-welcome-actions">
                        <button
                          type="button"
                          className="lng-link-btn"
                          onClick={() => openDialog({ kind: 'editChannel', guildId, channelId })}
                        >
                          <Icon name="edit" size={16} /> {t('lounge.messageList.editChannel')}
                        </button>
                      </div>
                    )}
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
                    aria-label={t('lounge.messageList.newMessagesDivider')}
                  >
                    <span>{t('lounge.messageList.newBadge')}</span>
                  </div>
                )}
                {row.kind === 'hidden' && (
                  <div className="lng-hidden-run">
                    <span>
                      {t('lounge.messageList.hiddenRun', {
                        count: row.count,
                        kind:
                          row.blocked && row.ignored ? 'both' : row.blocked ? 'blocked' : 'ignored',
                      })}
                    </span>
                    <button
                      type="button"
                      className="lng-link-btn"
                      onClick={() => setShownRuns((s) => new Set(s).add(row.key))}
                    >
                      {t('lounge.messageList.showHidden', { count: row.count })}
                    </button>
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
                    onMarkUnread={actions.markUnread ? onMarkUnread : undefined}
                    bookmarked={!!bookmarks[row.message.id]}
                    onBookmark={onBookmark}
                    onRemind={onRemind}
                    onCreateThread={canThread ? onCreateThread : undefined}
                    onOpenThread={guildId ? onOpenThread : undefined}
                    onJumpToChannelMessage={
                      guildId ? (c, id) => gotoMessage(guildId, c, id) : undefined
                    }
                    nameColor={roleColor.get(row.message.author.id)}
                    nameDecor={roleDecor.get(row.message.author.id)}
                    resolvers={resolvers}
                    isOwn={row.message.author.id === me.id}
                    onReply={onReply}
                    onEdit={onEdit}
                    onReact={onReact}
                    onMentionClick={onMention}
                    onOpenProfile={onOpenProfile}
                    onJumpTo={jumpTo}
                  />
                )}
              </div>
            );
          })}
        </div>
        {loadingOlder && (
          <div className="lng-loading-older" role="status">
            {t('lounge.messageList.loadingEarlier')}
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
          {t('lounge.messageList.viewingOlder')}
          <span>
            {t('lounge.messageList.jumpToPresent')} <Icon name="chevron-down" size={16} />
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
