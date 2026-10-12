import { EmptyState, Icon, useToast } from '@sideroom/ui';
import { useState } from 'react';
import type { DragEvent } from 'react';
import { ChannelHeader } from './ChannelHeader';
import { CHANNEL_ICON } from './ChannelSidebar';
import { Composer } from './Composer';
import { dmTitle, HOME } from './channels';
import {
  useChannels,
  useCurrentUser,
  useDms,
  useMessages,
  usePresence,
  useUsers,
  type Message,
} from './data';
import { MessageList } from './MessageList';
import { useLoungeUi } from './store';

export interface ChannelViewProps {
  guildId: string;
  channelId: string;
  onBack?: (() => void) | undefined;
}

/** One channel or DM: header, message list, composer. Remount (key) per channel. */
export function ChannelView({ guildId, channelId, onBack }: ChannelViewProps) {
  const isDm = guildId === HOME;
  const channels = useChannels(isDm ? undefined : guildId);
  const dms = useDms();
  const users = useUsers();
  const me = useCurrentUser();
  const { messages } = useMessages(channelId);
  const membersOpen = useLoungeUi((s) => s.memberListOpen);
  const setMembersOpen = useLoungeUi((s) => s.setMemberListOpen);
  const toast = useToast();

  const channel = channels.find((c) => c.id === channelId);
  const dm = dms.find((d) => d.id === channelId);
  const dmFirst = dm ? users[dm.recipientIds[0] ?? ''] : undefined;
  const presence = usePresence(dmFirst?.id ?? '');

  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [dragging, setDragging] = useState(false);
  // The NEW divider is anchored to where the viewer left off when they opened the channel.
  const [openedLastRead] = useState(channel?.lastReadMessageId ?? null);
  // Read state is acked by MessageList, once the newest message is actually on screen (Q128).

  if (!channel && !dm) {
    return (
      <EmptyState
        title="This channel doesn't exist"
        description="It may have been deleted, or you may not have access."
      />
    );
  }

  const title =
    isDm && dm ? dmTitle(dm, (id) => users[id]?.displayName ?? 'Unknown') : (channel?.name ?? '');
  const target = isDm ? title : `#${title}`;

  const editLast = () => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m && m.author.id === me.id && !m.pending) {
        setEditing(m);
        return;
      }
    }
  };

  const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes('Files');
  const drag = {
    onDragOver: (e: DragEvent) => {
      if (hasFiles(e)) {
        e.preventDefault();
        setDragging(true);
      }
    },
    onDragLeave: (e: DragEvent) => {
      if (e.currentTarget === e.target) setDragging(false);
    },
    onDrop: (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      setDragging(false);
      toast.show({ title: 'Uploads are not available yet', variant: 'info', duration: 2500 });
    },
  };

  return (
    <section className="lng-channel-view" aria-label={target} {...drag}>
      <ChannelHeader
        icon={channel ? CHANNEL_ICON[channel.type] : 'user'}
        title={title}
        isDm={isDm}
        status={isDm && dm && dm.recipientIds.length === 1 ? presence.status : undefined}
        topic={channel?.topic}
        membersOpen={membersOpen}
        onToggleMembers={() => setMembersOpen(!membersOpen)}
        onBack={onBack}
        searchLabel={`Search ${target}`}
        guildId={guildId}
        channelId={channelId}
      />
      <MessageList
        channelId={channelId}
        guildId={isDm ? undefined : guildId}
        title={title}
        isDm={isDm}
        lastReadId={openedLastRead}
        onReply={(m) => {
          setEditing(null);
          setReplyTo(m);
        }}
        onEdit={(m) => {
          setReplyTo(null);
          setEditing(m);
        }}
      />
      <Composer
        key={channelId}
        channelId={channelId}
        target={target}
        canSend={channel ? channel.canSend : true}
        replyTo={replyTo}
        onCancelReply={() => setReplyTo(null)}
        editing={editing}
        onCancelEdit={() => setEditing(null)}
        onEditLast={editLast}
      />
      {dragging && (
        <div className="lng-drop" aria-hidden="true">
          <Icon name="upload" size={24} />
          <strong>Upload to {target}</strong>
        </div>
      )}
    </section>
  );
}
