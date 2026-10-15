import { Avatar, Icon, Modal, Tooltip, type IconName } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { NotifyButton } from './header/NotifyButton';
import { InboxButton } from './inbox/InboxButton';
import { PinsButton } from './header/PinsButton';
import { SearchBox } from './SearchBox';
import { ThreadsButton } from './threads/ThreadsButton';

export interface ChannelHeaderProps {
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
      {isDm ? (
        <Avatar name={title} size={24} {...(status ? { status } : {})} />
      ) : (
        <Icon name={icon} size={24} />
      )}
      <h1 className="lng-header-title">{title}</h1>
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
        {!isDm && guildId && channelId && <ThreadsButton guildId={guildId} channelId={channelId} />}
        {guildId && channelId ? (
          <>
            <NotifyButton guildId={guildId} channelId={channelId} />
            <PinsButton guildId={guildId} channelId={channelId} canUnpin={!!canUnpin} />
          </>
        ) : (
          <>
            <Soon icon="bell" label={t('lounge.channelHeader.notificationSettings')} />
            <Soon icon="pin" label={t('lounge.channelHeader.pinnedMessages')} />
          </>
        )}
        <Tooltip
          content={
            membersOpen
              ? t('lounge.channelHeader.hideMembers')
              : t('lounge.channelHeader.showMembers')
          }
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.channelHeader.memberList')}
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
