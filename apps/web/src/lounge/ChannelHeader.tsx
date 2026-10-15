import { Avatar, Icon, Modal, Tooltip, type IconName } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { FollowButton } from './header/FollowButton';
import { NotifyButton } from './header/NotifyButton';
import { InboxButton } from './inbox/InboxButton';
import { SavedButton } from './features/SavedButton';
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

export function ChannelHeader({
  dm,
  icon,
  title,
  status,
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
  return (
    <header className="lng-header">
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
          <Avatar name={title} size={24} {...(status ? { status } : {})} />
          <span className="lng-header-title" role="heading" aria-level={1}>
            {title}
          </span>
        </button>
      ) : (
        <>
          {isDm ? (
            <Avatar name={title} size={24} {...(status ? { status } : {})} />
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
      <div className="lng-header-tools">
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
        {!isDm && followable && channelId && <FollowButton channelId={channelId} />}
        {!isDm && guildId && channelId && <ThreadsButton guildId={guildId} channelId={channelId} />}
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
            onClick={onToggleMembers}
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
        <Soon icon="info" label={t('lounge.channelHeader.help')} />
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
