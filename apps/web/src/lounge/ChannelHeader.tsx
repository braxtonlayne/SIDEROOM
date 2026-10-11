import { Avatar, Icon, Modal, Tooltip, type IconName } from '@sideroom/ui';
import { useState } from 'react';

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
}

function Soon({ icon, label }: { icon: IconName; label: string }) {
  return (
    <Tooltip content={`${label} (coming soon)`}>
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
}: ChannelHeaderProps) {
  const [topicOpen, setTopicOpen] = useState(false);
  return (
    <header className="lng-header">
      {onBack && (
        <button
          type="button"
          className="lng-icon-btn lng-back"
          aria-label="Back to channels"
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
        {!isDm && <Soon icon="list" label="Threads" />}
        <Soon icon="bell" label="Notification Settings" />
        <Soon icon="pin" label="Pinned Messages" />
        <Tooltip content={membersOpen ? 'Hide Member List' : 'Show Member List'}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Member List"
            aria-pressed={membersOpen}
            onClick={onToggleMembers}
          >
            <Icon name="users" size={20} />
          </button>
        </Tooltip>
        <label className="lng-search">
          <Icon name="search" size={16} />
          <input
            type="search"
            placeholder="Search"
            aria-label={searchLabel}
            readOnly
            title="Search ships with the search inventory items"
          />
        </label>
        <Soon icon="bookmark" label="Inbox" />
        <Soon icon="info" label="Help" />
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
