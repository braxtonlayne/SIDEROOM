import { Markdown, type Resolvers } from '@sideroom/markdown-react';
import {
  Avatar,
  ContextMenu,
  DropdownMenu,
  Icon,
  Popover,
  Tooltip,
  type MenuEntry,
} from '@sideroom/ui';
import { memo, useMemo, type CSSProperties } from 'react';
import { useT } from '../i18n';
import { useReportEntry } from '../moderation/reports';
import { useDeveloperMode, useSavedPrefs } from '../settings/user/prefs';
import { EmojiPicker } from './components/EmojiPicker';
import type { Message, Snowflake } from './data';
import { isJumbo, parseMarkdown, useMarkdownReady } from './markdown';
import { AttachmentList } from './msg/Attachments';
import { EphemeralNote } from './apps/EphemeralNote';
import { useAppsMenu } from './apps/useAppsMenu';
import { ComponentRows } from './msg/Components';
import { findInviteCode, InviteCard } from './msg/InviteCard';
import { findMessageLinks, MessageLinkCard } from './msg/MessageLinkCard';
import { EmbedList } from './msg/Embeds';
import { ForwardedFrame } from './msg/Forwarded';
import { InlineEdit } from './msg/InlineEdit';
import { PollView } from './msg/Poll';
import { ReactionRow } from './msg/Reactions';
import { REMIND_CHOICES } from './shell/savedMessages';
import { Sticker } from './msg/Sticker';
import { isSystemMessage, SystemMessage } from './msg/SystemMessage';
import { formatClock, formatFullTime, formatMessageTime } from './time';
import { ServerTagChip } from './components/ServerTagChip';

/** Name styling taken from the author's roles (LNG-MSGUI-002). */
export interface NameDecor {
  color?: string;
  /** Second colour of a gradient role. */
  secondary?: string;
  /** Role icon emoji and the role it comes from. */
  icon?: string;
  iconName?: string;
}

/** How the reply preview line resolves its target. */
export type ReplyState = 'loaded' | 'deleted' | 'unloaded';

export interface MessageItemProps {
  message: Message;
  grouped: boolean;
  /** Message this one replies to, if loaded. */
  replyTarget?: Message | undefined;
  replyState?: ReplyState | undefined;
  /** CSS color of the author's highest colored role. */
  nameColor?: string | undefined;
  nameDecor?: NameDecor | undefined;
  resolvers: Resolvers;
  isOwn: boolean;
  /** Undefined in DMs. */
  guildId?: Snowflake | undefined;
  /** The message mentions the viewer (directly, via a role, or @everyone). */
  mentionsMe?: boolean | undefined;
  /** Viewer may delete others' messages. */
  canManage?: boolean | undefined;
  canPin?: boolean | undefined;
  editing?: boolean | undefined;
  onReply: (m: Message) => void;
  onEdit: (m: Message) => void;
  onReact: (m: Message, emoji: string) => void;
  onMentionClick?: (t: { kind: 'user' | 'role' | 'channel'; id: Snowflake }) => void;
  onJumpTo?: (id: Snowflake) => void;
  /** `skipConfirm` is set when delete was shift-clicked. */
  onDelete?: (m: Message, skipConfirm: boolean) => void;
  onPin?: (m: Message) => void;
  onForward?: (m: Message) => void;
  onViewReactions?: (m: Message, emoji?: string) => void;
  onSaveEdit?: (m: Message, content: string) => void;
  onCancelEdit?: () => void;
  onVote?: (m: Message, answerId: number, add: boolean) => void;
  onEndPoll?: (m: Message) => void;
  onRetry?: (m: Message) => void;
  onDiscard?: (m: Message) => void;
  /** Mark Unread from this message. */
  onMarkUnread?: (m: Message) => void;
  bookmarked?: boolean | undefined;
  onBookmark?: (m: Message) => void;
  /** Remind Me after `ms` milliseconds. */
  onRemind?: (m: Message, ms: number) => void;
  /** Start a thread from this message (hidden when the viewer cannot, or in a thread). */
  onCreateThread?: (m: Message) => void;
  onOpenThread?: (m: Message, threadId: Snowflake) => void;
  /** Open the author's profile popout next to `anchor`. */
  onOpenProfile?: (userId: Snowflake, anchor: HTMLElement) => void;
  /** Jump to a message in another channel (the first row of a thread). */
  onJumpToChannelMessage?: (channelId: Snowflake, messageId: Snowflake) => void;
}

const QUICK = ['👍', '😂', '🎉'];
const MENU_EMOJI = ['👍', '❤️', '😂', '😮', '😢', '🎉'];

/** Message permalink used by Copy Message Link. */
export function messageLink(origin: string, guildId: Snowflake | undefined, m: Message): string {
  return `${origin}/channels/${guildId ?? '@me'}/${m.channelId}/${m.id}`;
}

function MessageBody({
  m,
  guildId,
  resolvers,
  isOwn,
  onMentionClick,
  onVote,
  onEndPoll,
}: Pick<MessageItemProps, 'resolvers' | 'onMentionClick' | 'isOwn' | 'guildId'> & {
  m: Message;
  onVote?: (answerId: number, add: boolean) => void;
  onEndPoll?: () => void;
}) {
  // Re-renders this body once the WASM parser is ready (plain text until then).
  useMarkdownReady();
  const t = useT();
  const { linkPreviews, showEmbeds, spoilers } = useSavedPrefs();
  const nodes = parseMarkdown(m.content);
  // Link previews (everything built from a pasted link) and embeds posted by apps are separate switches.
  const embeds = (m.embeds ?? []).filter((e) => (e.type === 'rich' ? showEmbeds : linkPreviews));
  const showCards = linkPreviews && !m.suppressEmbeds;
  const inviteCode = useMemo(() => findInviteCode(m.content, location.origin), [m.content]);
  const messageLinks = useMemo(() => findMessageLinks(m.content, location.origin), [m.content]);
  return (
    <>
      {m.content && (
        <Markdown
          nodes={nodes}
          jumbo={isJumbo(m.content)}
          revealSpoilers={spoilers === 'always'}
          resolvers={resolvers}
          {...(onMentionClick ? { onMentionClick } : {})}
        />
      )}
      {m.editedAt && (
        <span className="lng-edited" title={formatFullTime(m.editedAt)}>
          {' '}
          {t('lounge.messageItem.edited')}
        </span>
      )}
      {m.forwarded && <ForwardedFrame snapshot={m.forwarded} resolvers={resolvers} />}
      {m.attachments && m.attachments.length > 0 && (
        <AttachmentList attachments={m.attachments} voice={!!m.voice} />
      )}
      {embeds.length > 0 && !m.suppressEmbeds && (
        <EmbedList embeds={embeds} resolvers={resolvers} />
      )}
      {showCards && inviteCode && <InviteCard code={inviteCode} />}
      {showCards && messageLinks.map((l) => <MessageLinkCard key={l.messageId} link={l} />)}
      {m.components && m.components.length > 0 && (
        <ComponentRows
          rows={m.components}
          guildId={guildId}
          context={m.ephemeral ? undefined : { channelId: m.channelId, messageId: m.id }}
        />
      )}
      {m.ephemeral && <EphemeralNote channelId={m.channelId} messageId={m.id} />}
      {m.stickers?.map((s) => (
        <Sticker key={s.id} sticker={s} guildId={guildId} />
      ))}
      {m.poll && (
        <PollView
          poll={m.poll}
          channelId={m.channelId}
          messageId={m.id}
          isOwn={isOwn}
          onVote={(a, add) => onVote?.(a, add)}
          onEnd={onEndPoll}
        />
      )}
    </>
  );
}

function authorStyle(color?: string, decor?: NameDecor): CSSProperties | undefined {
  if (decor?.secondary && (decor.color ?? color)) {
    return {
      backgroundImage: `linear-gradient(90deg, ${decor.color ?? color}, ${decor.secondary})`,
    };
  }
  return color ? { color } : undefined;
}

function MessageItemImpl(props: MessageItemProps) {
  const {
    message: m,
    grouped: groupedProp,
    replyTarget,
    replyState,
    nameColor,
    nameDecor,
    isOwn,
    guildId,
    mentionsMe,
    canManage,
    canPin,
    editing,
    onReply,
    onEdit,
    onReact,
    onJumpTo,
    onDelete,
    onPin,
    onForward,
    onViewReactions,
    onSaveEdit,
    onCancelEdit,
    onVote,
    onEndPoll,
    onRetry,
    onDiscard,
    onMarkUnread,
    bookmarked,
    onBookmark,
    onRemind,
    onCreateThread,
    onOpenThread,
    onJumpToChannelMessage,
    onOpenProfile,
  } = props;
  const t = useT();
  const reportEntry = useReportEntry();
  const prefs = useSavedPrefs();
  const compact = prefs.messageDisplay === 'compact';
  // Role colours: on the name (default), as a dot before it, or off (User Settings > Accessibility).
  const roleColorOn = prefs.roleColors === 'name';
  // Compact mode puts the name and time on every line (no avatar), so nothing is grouped.
  const grouped = groupedProp && !compact;

  const appsMenu = useAppsMenu(
    3,
    { guildId, channelId: m.ephemeral ? undefined : m.channelId },
    m.id,
  );
  const canDelete = !!onDelete && (isOwn || !!canManage) && !m.pending;
  const canPinHere = !!onPin && !!canPin && !m.pending;
  const copy = (text: string) => void navigator.clipboard?.writeText(text);

  const developerMode = useDeveloperMode();
  const menu = useMemo<MenuEntry[]>(() => {
    const entries: MenuEntry[] = [
      {
        type: 'submenu',
        label: t('lounge.messageItem.addReaction'),
        icon: <Icon name="smile" size={16} />,
        items: MENU_EMOJI.map((e) => ({ label: e, onSelect: () => onReact(m, e) })),
      },
    ];
    if (isOwn)
      entries.push({ label: t('lounge.messageItem.editMessage'), onSelect: () => onEdit(m) });
    entries.push({ label: t('lounge.messageItem.reply'), onSelect: () => onReply(m) });
    if (onForward)
      entries.push({ label: t('lounge.messageItem.forward'), onSelect: () => onForward(m) });
    if (m.thread && onOpenThread)
      entries.push({
        label: t('lounge.messageItem.openThread'),
        onSelect: () => onOpenThread(m, m.thread!.id),
      });
    else if (onCreateThread && !m.pending)
      entries.push({
        label: t('lounge.messageItem.createThread'),
        onSelect: () => onCreateThread(m),
      });
    if (m.content)
      entries.push({ label: t('lounge.messageItem.copyText'), onSelect: () => copy(m.content) });
    if (m.reactions?.length && onViewReactions)
      entries.push({
        label: t('lounge.messageItem.viewReactions'),
        onSelect: () => onViewReactions(m),
      });
    if (m.content && typeof speechSynthesis !== 'undefined')
      entries.push({
        label: t('lounge.messageItem.speakMessage'),
        onSelect: () => {
          speechSynthesis.cancel();
          speechSynthesis.speak(
            new SpeechSynthesisUtterance(
              t('lounge.messageItem.speechText', {
                name: m.author.displayName,
                content: m.content,
              }),
            ),
          );
        },
      });
    if (onMarkUnread && !m.pending)
      entries.push({ label: t('lounge.messageItem.markUnread'), onSelect: () => onMarkUnread(m) });
    entries.push({
      label: t('lounge.messageItem.copyMessageLink'),
      onSelect: () => copy(messageLink(location.origin, guildId, m)),
    });
    if (onBookmark && !m.pending)
      entries.push({
        label: bookmarked
          ? t('lounge.messageItem.removeBookmark')
          : t('lounge.messageItem.bookmarkMessage'),
        onSelect: () => onBookmark(m),
      });
    if (onRemind && !m.pending)
      entries.push({
        type: 'submenu',
        label: t('lounge.messageItem.remindMe'),
        items: REMIND_CHOICES.map((c) => ({
          label: c.label,
          onSelect: () => onRemind(m, c.ms),
        })),
      });
    if (canPinHere)
      entries.push({
        label: m.pinned ? t('lounge.messageItem.unpinMessage') : t('lounge.messageItem.pinMessage'),
        onSelect: () => onPin!(m),
      });
    if (canDelete) {
      entries.push({ type: 'separator' });
      entries.push({
        label: t('lounge.messageItem.deleteMessage'),
        danger: true,
        onSelect: () => onDelete!(m, false),
      });
    }
    if (appsMenu.length > 0) entries.push({ type: 'separator' }, ...appsMenu);
    const report = reportEntry({ kind: 'message', id: m.id });
    if (report) entries.push({ type: 'separator' }, report);
    if (developerMode) {
      entries.push({ type: 'separator' });
      entries.push({ label: t('lounge.messageItem.copyMessageId'), onSelect: () => copy(m.id) });
    }
    return entries;
  }, [
    t,
    developerMode,
    m,
    isOwn,
    guildId,
    appsMenu,
    canPinHere,
    canDelete,
    onReact,
    onEdit,
    onReply,
    onForward,
    onViewReactions,
    onPin,
    onDelete,
    onMarkUnread,
    bookmarked,
    onBookmark,
    onRemind,
    onCreateThread,
    onOpenThread,
    reportEntry,
  ]);

  if (isSystemMessage(m)) {
    return (
      <SystemMessage
        message={m}
        referenceId={m.replyToId}
        referenceChannelId={m.referenceChannelId}
        {...(onJumpTo ? { onJumpTo } : {})}
        {...(onOpenThread ? { onOpenThread: (id: Snowflake) => onOpenThread(m, id) } : {})}
        onJumpToChannelMessage={onJumpToChannelMessage}
      />
    );
  }

  const toolbar = !m.pending && !m.failed && !editing && (
    <div
      className="lng-msg-tools"
      role="toolbar"
      aria-label={t('lounge.messageItem.toolbarLabel', { name: m.author.displayName })}
    >
      {QUICK.map((e) => (
        <button
          key={e}
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.messageItem.reactWith', { emoji: e })}
          onClick={() => onReact(m, e)}
        >
          <span aria-hidden="true">{e}</span>
        </button>
      ))}
      <Popover
        label={t('lounge.messageItem.addReactionPopover')}
        placement="left"
        trigger={
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.messageItem.addReaction')}
          >
            <Icon name="smile" size={20} />
          </button>
        }
      >
        <EmojiPicker onPick={(e) => onReact(m, e)} />
      </Popover>
      <Tooltip content={t('lounge.messageItem.reply')}>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.messageItem.reply')}
          onClick={() => onReply(m)}
        >
          <Icon name="reply" size={20} />
        </button>
      </Tooltip>
      {onForward && (
        <Tooltip content={t('lounge.messageItem.forward')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.messageItem.forward')}
            onClick={() => onForward(m)}
          >
            <Icon name="arrow-up-right" size={20} />
          </button>
        </Tooltip>
      )}
      {onCreateThread && !m.thread && (
        <Tooltip content={t('lounge.messageItem.createThread')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.messageItem.createThread')}
            onClick={() => onCreateThread(m)}
          >
            <Icon name="list" size={20} />
          </button>
        </Tooltip>
      )}
      {isOwn && (
        <Tooltip content={t('lounge.messageItem.edit')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.messageItem.edit')}
            onClick={() => onEdit(m)}
          >
            <Icon name="edit" size={20} />
          </button>
        </Tooltip>
      )}
      {canDelete && (
        <Tooltip content={t('lounge.messageItem.deleteTooltip')}>
          <button
            type="button"
            className="lng-icon-btn lng-danger"
            aria-label={t('lounge.messageItem.delete')}
            onClick={(e) => onDelete!(m, e.shiftKey)}
          >
            <Icon name="trash" size={20} />
          </button>
        </Tooltip>
      )}
      <DropdownMenu
        label={t('lounge.messageItem.more')}
        items={menu}
        placement="bottom-end"
        trigger={
          <button type="button" className="lng-icon-btn" aria-label={t('lounge.messageItem.more')}>
            <Icon name="more-horizontal" size={20} />
          </button>
        }
      />
    </div>
  );

  return (
    <ContextMenu
      items={menu}
      label={t('lounge.messageItem.contextMenuLabel', { name: m.author.displayName })}
    >
      <article
        className="lng-msg"
        data-grouped={grouped || undefined}
        data-pending={m.pending || undefined}
        data-failed={m.failed || undefined}
        data-mention={mentionsMe || undefined}
        data-pinned={m.pinned || undefined}
        data-editing={editing || undefined}
        data-compact={compact || undefined}
        data-message-id={m.id}
        aria-label={t('lounge.messageItem.articleLabel', {
          name: m.author.displayName,
          time: formatMessageTime(m.createdAt),
        })}
      >
        {m.replyToId && (
          <button
            type="button"
            className="lng-reply"
            onClick={() => onJumpTo?.(m.replyToId!)}
            disabled={replyState === 'deleted'}
          >
            <Icon name="reply" size={16} />
            {replyTarget ? (
              <>
                <Avatar
                  name={replyTarget.author.displayName}
                  src={replyTarget.author.avatarUrl ?? null}
                  size={24}
                  className="lng-reply-avatar"
                  style={{ width: 16, height: 16, fontSize: 8 }}
                  aria-hidden="true"
                />
                <span className="lng-reply-name">@{replyTarget.author.displayName}</span>
                <span className="lng-reply-text">
                  {replyTarget.content.slice(0, 120) ||
                    t('lounge.messageItem.clickToSeeAttachment')}
                </span>
              </>
            ) : replyState === 'deleted' ? (
              <span className="lng-reply-text lng-muted">
                {t('lounge.messageItem.originalDeleted')}
              </span>
            ) : (
              <span className="lng-reply-text lng-muted">
                {t('lounge.messageItem.clickToLoadOriginal')}
              </span>
            )}
          </button>
        )}
        <div className="lng-msg-main">
          <div className="lng-msg-gutter">
            {grouped || compact ? (
              <time
                className="lng-msg-hover-time"
                dateTime={m.createdAt}
                title={formatFullTime(m.createdAt)}
              >
                {formatClock(new Date(m.createdAt))}
              </time>
            ) : (
              <button
                type="button"
                className="lng-avatar-btn"
                aria-label={t('lounge.messageItem.openProfile', { name: m.author.displayName })}
                aria-haspopup="dialog"
                disabled={!onOpenProfile}
                onClick={(e) => onOpenProfile?.(m.author.id, e.currentTarget)}
              >
                <Avatar
                  name={m.author.displayName}
                  src={m.author.avatarUrl ?? null}
                  size={40}
                  aria-hidden="true"
                />
              </button>
            )}
          </div>
          <div className="lng-msg-body">
            {!grouped && (
              <header className="lng-msg-head">
                <span
                  className="lng-msg-author"
                  data-gradient={roleColorOn && nameDecor?.secondary ? '' : undefined}
                  style={roleColorOn ? authorStyle(nameColor, nameDecor) : undefined}
                >
                  {prefs.roleColors === 'dot' && nameColor && (
                    <span
                      className="lng-role-dot"
                      aria-hidden="true"
                      style={{ background: nameColor }}
                    />
                  )}
                  {onOpenProfile ? (
                    <button
                      type="button"
                      className="lng-name-btn"
                      aria-haspopup="dialog"
                      onClick={(e) => onOpenProfile(m.author.id, e.currentTarget)}
                    >
                      {m.author.displayName}
                    </button>
                  ) : (
                    m.author.displayName
                  )}
                </span>
                {nameDecor?.icon && (
                  <span
                    className="lng-role-icon"
                    role="img"
                    aria-label={t('lounge.messageItem.roleIcon', {
                      name: nameDecor.iconName ?? 'Role',
                    })}
                    title={nameDecor.iconName}
                  >
                    {nameDecor.icon}
                  </span>
                )}
                <ServerTagChip user={m.author} />
                {m.author.bot && (
                  <span className="lng-bot">{t('lounge.messageItem.appBadge')}</span>
                )}
                {m.author.system && (
                  <span className="lng-bot">{t('lounge.messageItem.systemBadge')}</span>
                )}
                {!compact && (
                  <time
                    className="lng-msg-time"
                    dateTime={m.createdAt}
                    title={formatFullTime(m.createdAt)}
                  >
                    {formatMessageTime(m.createdAt)}
                  </time>
                )}
              </header>
            )}
            <div className="lng-msg-content">
              {editing && onSaveEdit && onCancelEdit ? (
                <InlineEdit
                  initial={m.content}
                  onSave={(c) => onSaveEdit(m, c)}
                  onCancel={onCancelEdit}
                  onEmpty={() => onDelete?.(m, false)}
                />
              ) : (
                <MessageBody
                  m={m}
                  guildId={guildId}
                  isOwn={isOwn}
                  resolvers={props.resolvers}
                  {...(props.onMentionClick ? { onMentionClick: props.onMentionClick } : {})}
                  onVote={(a, add) => onVote?.(m, a, add)}
                  {...(onEndPoll && isOwn ? { onEndPoll: () => onEndPoll(m) } : {})}
                />
              )}
            </div>
            {m.failed && (
              <p className="lng-failed" role="alert">
                {t('lounge.messageItem.deliveryFailed')}{' '}
                {onRetry && (
                  <button type="button" className="lng-link-btn" onClick={() => onRetry(m)}>
                    {t('lounge.messageItem.retry')}
                  </button>
                )}{' '}
                {onDiscard && (
                  <button
                    type="button"
                    className="lng-link-btn lng-danger"
                    onClick={() => onDiscard(m)}
                  >
                    {t('lounge.messageItem.delete')}
                  </button>
                )}
              </p>
            )}
            {m.thread && onOpenThread && (
              <button
                type="button"
                className="lng-thread-chip"
                onClick={() => onOpenThread(m, m.thread!.id)}
              >
                <Icon name="list" size={16} />
                <strong>{m.thread.name}</strong>
                <span className="lng-muted">
                  {t('lounge.messageItem.threadMessageCount', { n: m.thread.messageCount })}
                </span>
              </button>
            )}
            {prefs.showReactions && (
              <ReactionRow message={m} onReact={onReact} onViewReactions={onViewReactions} />
            )}
          </div>
        </div>
        {toolbar}
      </article>
    </ContextMenu>
  );
}

export const MessageItem = memo(MessageItemImpl);
