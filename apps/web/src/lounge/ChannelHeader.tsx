import { Avatar, Icon, Modal, Tooltip, type IconName } from '@sideroom/ui';
import { useEffect, useState, type KeyboardEvent } from 'react';
import { useT } from '../i18n';
import { statusProps } from './phoneStatus';
import { FollowButton } from './header/FollowButton';
import { NotifyButton } from './header/NotifyButton';
import { InboxButton } from './inbox/InboxButton';
import { SavedButton } from './features/SavedButton';
import { HelpButton } from './header/HelpButton';
import { PhoneSearch } from './header/PhoneSearch';
import { PinsButton } from './header/PinsButton';
import { SearchBox } from './SearchBox';
import { ThreadsButton } from './threads/ThreadsButton';

/** What a direct-message header adds (LNG-DM-004). */
export interface DmHeaderActions {
  /** Group DM: the identity opens the member panel instead of a profile. */
  group: boolean;
  onIdentity(anchor: HTMLElement): void;
  onCall(): void;
  onVideo(): void;
  onAddFriends(): void;
  /** Disable the call buttons (already in a call here, or the viewer cannot call). */
  callDisabled?: boolean;
}

export interface ChannelHeaderProps {
  dm?: DmHeaderActions | undefined;
  icon: IconName;
  title: string;
  /** Presence status for DM headers. */
  status?: 'online' | 'idle' | 'dnd' | 'offline' | undefined;
  /** The other person is online on a phone alone (D-702): their status shows the phone glyph. */
  phone?: boolean | undefined;
  isDm?: boolean;
  topic?: string | null | undefined;
  membersOpen: boolean;
  onToggleMembers: () => void;
  onBack?: (() => void) | undefined;
  /** "Search #general" / "Search Mara". */
  searchLabel: string;
  /** Guild (undefined or '@me' for a DM) and channel the search box queries. */
  guildId?: string | undefined;
  channelId?: string | undefined;
  /** The viewer may unpin messages (shown in the pins popout). */
  canUnpin?: boolean | undefined;
  /** Small label after the name for Community channels: "Rules", "Announcements". */
  badge?: string | undefined;
  /** An announcement channel: show the Follow button (G-30). */
  followable?: boolean | undefined;
}

function Soon({ icon, label }: { icon: IconName; label: string }) {
  const t = useT();
  return (
    <Tooltip content={t('lounge.channelHeader.comingSoon', { label })}>
      <button
        type="button"
        className="lng-icon-btn"
        aria-label={label}
        aria-disabled="true"
        onClick={(e) => e.preventDefault()}
      >
        <Icon name={icon} size={20} />
      </button>
    </Tooltip>
  );
}

/**
 * D-1014: while the header is short of room the search field is folded to its icon and open only while it or its panel
 * has focus (the stylesheet sets `--lng-search-folded` on the tools then). Escape closes the panel (SearchBox); this moves
 * focus off the field, to the tool before it, so the field folds again and the Tab order is not lost.
 */
function foldSearchOnEscape(e: KeyboardEvent<HTMLDivElement>) {
  if (e.key !== 'Escape' || !(e.target instanceof HTMLInputElement)) return;
  const tools = e.currentTarget;
  if (getComputedStyle(tools).getPropertyValue('--lng-search-folded').trim() !== '1') return;
  if (!e.target.closest('.lng-search-wrap')) return;
  const before = Array.from(tools.querySelectorAll('button')).filter(
    (b) =>
      !b.closest('.lng-search-wrap') &&
      !!(e.target as Node).compareDocumentPosition(b) &&
      (e.target as Node).compareDocumentPosition(b) & Node.DOCUMENT_POSITION_PRECEDING,
  );
  const prev = before[before.length - 1];
  if (prev) prev.focus();
  else e.target.blur();
}

export function ChannelHeader({
  dm,
  icon,
  title,
  status,
  phone,
  isDm,
  topic,
  membersOpen,
  onToggleMembers,
  onBack,
  searchLabel,
  guildId,
  channelId,
  canUnpin,
  badge,
  followable,
}: ChannelHeaderProps) {
  const t = useT();
  const [topicOpen, setTopicOpen] = useState(false);
  // SEC-AUD-164: on a phone a DM header keeps back, the name, and the two call buttons; the other controls open
  // from a "More" button, as the same buttons (nothing is removed, popovers keep their own state).
  const [moreOpen, setMoreOpen] = useState(false);
  const dmHeader = !!(isDm && dm);
  // SEC-AUD-368 (D-872): onBack is only given at phone width; a channel's header there is compact too.
  const compact = !!(onBack && !dmHeader);
  useEffect(() => {
    if (!moreOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMoreOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [moreOpen]);
  return (
    <header
      className="lng-header"
      data-dm={dmHeader || undefined}
      data-compact={compact || undefined}
    >
      {onBack && (
        <button
          type="button"
          className="lng-icon-btn lng-back"
          aria-label={t('lounge.channelHeader.backToChannels')}
          onClick={onBack}
        >
          <Icon name="arrow-left" size={20} />
        </button>
      )}
      {isDm && dm ? (
        <button
          type="button"
          className="lng-dm-identity"
          aria-label={
            dm.group
              ? t('lounge.channelHeader.openGroup', { name: title })
              : t('lounge.channelHeader.openProfile', { name: title })
          }
          onClick={(e) => dm.onIdentity(e.currentTarget)}
        >
          <Avatar name={title} size={24} {...(status ? statusProps({ status, phone }) : {})} />
          <span className="lng-header-title" role="heading" aria-level={1}>
            {title}
          </span>
        </button>
      ) : (
        <>
          {isDm ? (
            <Avatar name={title} size={24} {...(status ? statusProps({ status, phone }) : {})} />
          ) : (
            <Icon name={icon} size={24} />
          )}
          <h1 className="lng-header-title">{title}</h1>
        </>
      )}
      {badge && <span className="lng-chan-badge">{badge}</span>}
      {topic && (
        <>
          <span className="lng-header-sep" aria-hidden="true" />
          <button
            type="button"
            className="lng-topic"
            title={topic}
            onClick={() => setTopicOpen(true)}
          >
            {topic}
          </button>
        </>
      )}
      <div className="lng-header-tools" onKeyDown={foldSearchOnEscape}>
        {isDm && dm && (
          <>
            <Tooltip content={t('lounge.channelHeader.startCall')}>
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.channelHeader.startCall')}
                disabled={dm.callDisabled}
                onClick={dm.onCall}
              >
                <Icon name="phone" size={20} />
              </button>
            </Tooltip>
            <Tooltip content={t('lounge.channelHeader.startVideoCall')}>
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.channelHeader.startVideoCall')}
                disabled={dm.callDisabled}
                onClick={dm.onVideo}
              >
                <Icon name="video" size={20} />
              </button>
            </Tooltip>
          </>
        )}
        {/* D-740: onBack is only given at phone width, where the inline search box is hidden. */}
        {onBack && channelId && (
          <PhoneSearch guildId={guildId} channelId={channelId} label={searchLabel} />
        )}
        {(dmHeader || compact) && (
          <button
            type="button"
            className="lng-icon-btn lng-dm-more"
            aria-label={t('lounge.channelHeader.moreActions')}
            aria-expanded={moreOpen}
            onClick={() => setMoreOpen((o) => !o)}
          >
            <Icon name="more-horizontal" size={20} />
          </button>
        )}
        <div className="lng-dm-extra" data-open={moreOpen || undefined}>
          {!isDm && followable && channelId && <FollowButton channelId={channelId} />}
          {!isDm && guildId && channelId && (
            <ThreadsButton guildId={guildId} channelId={channelId} />
          )}
          {guildId && channelId ? (
            <>
              <NotifyButton guildId={guildId} channelId={channelId} />
              <PinsButton guildId={guildId} channelId={channelId} canUnpin={!!canUnpin} />
              <SavedButton />
            </>
          ) : (
            <>
              <Soon icon="bell" label={t('lounge.channelHeader.notificationSettings')} />
              <Soon icon="pin" label={t('lounge.channelHeader.pinnedMessages')} />
            </>
          )}
          {isDm && dm && (
            <Tooltip content={t('lounge.channelHeader.addFriendsToDm')}>
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.channelHeader.addFriendsToDm')}
                onClick={dm.onAddFriends}
              >
                <Icon name="plus" size={20} />
              </button>
            </Tooltip>
          )}
          <Tooltip
            content={
              isDm && dm && !dm.group
                ? membersOpen
                  ? t('lounge.channelHeader.hideProfile')
                  : t('lounge.channelHeader.showProfile')
                : membersOpen
                  ? t('lounge.channelHeader.hideMembers')
                  : t('lounge.channelHeader.showMembers')
            }
          >
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={
                isDm && dm && !dm.group
                  ? t('lounge.channelHeader.userProfile')
                  : t('lounge.channelHeader.memberList')
              }
              aria-pressed={membersOpen}
              onClick={() => {
                onToggleMembers();
                // SEC-AUD-380 (D-892): on a phone the strip closes once the members pane is chosen.
                setMoreOpen(false);
              }}
            >
              <Icon name="users" size={20} />
            </button>
          </Tooltip>
          {channelId ? (
            <SearchBox guildId={guildId} channelId={channelId} label={searchLabel} />
          ) : (
            <label className="lng-search">
              <Icon name="search" size={16} />
              <input
                type="search"
                placeholder={t('lounge.channelHeader.search')}
                aria-label={searchLabel}
                readOnly
              />
            </label>
          )}
          <InboxButton />
          <HelpButton />
        </div>
      </div>
      <Modal
        open={topicOpen}
        onClose={() => setTopicOpen(false)}
        title={isDm ? title : `#${title}`}
        size="sm"
      >
        <p>{topic}</p>
      </Modal>
    </header>
  );
}
