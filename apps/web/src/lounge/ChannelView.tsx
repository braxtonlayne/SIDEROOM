import { EmptyState, Icon } from '@sideroom/ui';
import { useRef, useState } from 'react';
import type { DragEvent } from 'react';
import { useT } from '../i18n';
import { ChannelHeader } from './ChannelHeader';
import { CHANNEL_ICON } from './ChannelSidebar';
import { Composer, type ComposerHandle } from './Composer';
import { dmTitle, HOME } from './channels';
import {
  useChannels,
  useCurrentUser,
  useDms,
  useGuild,
  useMessages,
  usePresence,
  useUsers,
  type Message,
} from './data';
import { AgeConfirmPrompt, useAgeGate } from './AgeGate';
import { MessageList } from './MessageList';
import { ForumView } from './forum/ForumView';
import { VoiceChannelView } from './voice/VoiceChannelView';
import { WatchPartyStrip } from '../watchparty/WatchPartyStrip';
import { useLoungeGoto } from './nav';
import { useMyTimeout } from './timeout';
import { useLoungeUi } from './store';
import { openProfilePopout } from './profile/popoutStore';
import { startDmCall } from './shell/dmCall';
import { useVoiceSession } from '../voice/session';

export interface ChannelViewProps {
  guildId: string;
  channelId: string;
  onBack?: (() => void) | undefined;
}

/** One channel or DM: header, message list, composer. Remount (key) per channel. */
export function ChannelView({ guildId, channelId, onBack }: ChannelViewProps) {
  const t = useT();
  const isDm = guildId === HOME;
  const channels = useChannels(isDm ? undefined : guildId);
  const dms = useDms();
  const users = useUsers();
  const me = useCurrentUser();
  const { messages } = useMessages(channelId);
  const membersOpen = useLoungeUi((s) => s.memberListOpen);
  const setMembersOpen = useLoungeUi((s) => s.setMemberListOpen);
  const composer = useRef<ComposerHandle | null>(null);
  const guild = useGuild(isDm ? undefined : guildId);
  const timedOutUntil = useMyTimeout(isDm ? undefined : guildId);

  const channel = channels.find((c) => c.id === channelId);
  const gate = useAgeGate(channel?.nsfw);
  const goto = useLoungeGoto();
  const dm = dms.find((d) => d.id === channelId);
  const dmFirst = dm ? users[dm.recipientIds[0] ?? ''] : undefined;
  const presence = usePresence(dmFirst?.id ?? '');
  const openDialog = useLoungeUi((s) => s.openDialog);
  const inCallHere = useVoiceSession((s) => s.channelId === channelId && s.status !== 'idle');

  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [dragging, setDragging] = useState(false);
  // The NEW divider is anchored to where the viewer left off when they opened the channel.
  const [openedLastRead] = useState(channel?.lastReadMessageId ?? null);
  // Read state is acked by MessageList, once the newest message is actually on screen (Q128).

  if (!channel && !dm) {
    return (
      <EmptyState
        title={t('lounge.channelView.missingTitle')}
        description={t('lounge.channelView.missingDescription')}
      />
    );
  }

  // Server voice channels have their own view (tiles and call controls), not a message list.
  if (channel?.type === 'voice') return <VoiceChannelView channel={channel} onBack={onBack} />;

  const title =
    isDm && dm
      ? dmTitle(dm, (id) => users[id]?.displayName ?? t('lounge.channelView.unknown'))
      : (channel?.name ?? '');
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
      composer.current?.addFiles(Array.from(e.dataTransfer?.files ?? []));
    },
  };

  return (
    <section className="lng-channel-view" aria-label={target} {...drag}>
      <ChannelHeader
        dm={
          isDm && dm
            ? {
                group: dm.recipientIds.length > 1,
                callDisabled: inCallHere,
                onIdentity: (anchor) => {
                  if (dm.recipientIds.length > 1 || !dmFirst) setMembersOpen(true);
                  else openProfilePopout({ userId: dmFirst.id }, anchor);
                },
                onCall: () => startDmCall(channelId, title, false),
                onVideo: () => startDmCall(channelId, title, true),
                onAddFriends: () => openDialog({ kind: 'addToDm', channelId }),
              }
            : undefined
        }
        icon={channel ? CHANNEL_ICON[channel.type] : 'user'}
        title={title}
        isDm={isDm}
        status={isDm && dm && dm.recipientIds.length === 1 ? presence.status : undefined}
        topic={channel?.topic}
        membersOpen={membersOpen}
        onToggleMembers={() => setMembersOpen(!membersOpen)}
        onBack={onBack}
        searchLabel={t('lounge.channelView.search', { target })}
        guildId={guildId}
        channelId={channelId}
        badge={
          channel?.type === 'rules'
            ? t('lounge.channelView.badgeRules')
            : channel?.type === 'announcement'
              ? t('lounge.channelView.badgeAnnouncements')
              : undefined
        }
        canUnpin={
          channel?.permissions
            ? channel.permissions.pinMessages
            : isDm || !!guild?.permissions.pinMessages
        }
      />
      <WatchPartyStrip channelId={channelId} />
      {gate.state !== 'open' ? (
        <div className="lng-agegate-wrap">
          {gate.state === 'wait' ? (
            <p role="status" className="lng-muted">
              {t('lounge.channelView.checking')}
            </p>
          ) : gate.state === 'ask' ? (
            <AgeConfirmPrompt
              title={t('lounge.channelView.restrictedTitle', { title })}
              busy={gate.busy}
              error={gate.error}
              onAnswer={(over18) =>
                void gate.submit(over18).then((ok) => {
                  if (ok === false) goto(isDm ? HOME : guildId);
                })
              }
            />
          ) : (
            <EmptyState
              title={t('lounge.channelView.restrictedChannel')}
              description={t('lounge.channelView.restrictedDescription')}
            />
          )}
        </div>
      ) : channel && (channel.type === 'forum' || channel.type === 'media') ? (
        <ForumView guildId={guildId} channel={channel} />
      ) : (
        <>
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
            editingId={editing?.id}
            onCancelEdit={() => setEditing(null)}
          />
          <Composer
            key={channelId}
            ref={composer}
            channelId={channelId}
            guildId={isDm ? undefined : guildId}
            slowmodeSeconds={channel?.slowmodeSeconds}
            bypassSlowmode={
              channel?.permissions
                ? channel.permissions.manageMessages || channel.permissions.manageChannels
                : !!(guild?.permissions.manageMessages || guild?.permissions.manageChannels)
            }
            canMentionEveryone={
              isDm
                ? false
                : (channel?.permissions?.mentionEveryone ??
                  guild?.permissions.mentionEveryone ??
                  false)
            }
            target={target}
            timedOutUntil={timedOutUntil}
            canTts={!isDm && (channel?.permissions?.sendTts ?? true)}
            canStartThread={
              !isDm &&
              (channel?.type === 'text' || channel?.type === 'announcement') &&
              (channel.permissions?.createThreads ?? true)
            }
            canSend={channel ? channel.canSend : true}
            readOnlyNote={
              channel?.type === 'rules'
                ? t('lounge.channelView.readOnlyRules')
                : channel?.type === 'announcement'
                  ? t('lounge.channelView.readOnlyAnnouncements')
                  : undefined
            }
            replyTo={replyTo}
            onCancelReply={() => setReplyTo(null)}
            onEditLast={editLast}
          />
        </>
      )}
      {dragging && (
        <div className="lng-drop" aria-hidden="true">
          <Icon name="upload" size={24} />
          <strong>{t('lounge.channelView.uploadTo', { target })}</strong>
        </div>
      )}
    </section>
  );
}
