import { useRef, useState } from 'react';
import { Icon } from '@sideroom/ui';
import { useT } from '../../i18n';
import { Composer, type ComposerHandle } from '../Composer';
import { useGuild, useMessages, useCurrentUser, type Channel, type Message } from '../data';
import { MessageList } from '../MessageList';
import { useMyTimeout } from '../timeout';
import './voice-p10.css';

/**
 * Text-in-voice (VOX-UI-007): the voice channel's own text chat in a panel beside the call. Voice channels
 * accept messages like text channels (`ChannelType::accepts_messages`), so this is the ordinary message list
 * and composer for the channel id, with the same permissions, slowmode and timeouts.
 */
export function VoiceChat({ channel, onClose }: { channel: Channel; onClose: () => void }) {
  const t = useT();
  const guild = useGuild(channel.guildId);
  const me = useCurrentUser();
  const { messages } = useMessages(channel.id);
  const timedOutUntil = useMyTimeout(channel.guildId);
  const composer = useRef<ComposerHandle | null>(null);
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);

  const editLast = () => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m && m.author.id === me.id && !m.pending) {
        setEditing(m);
        return;
      }
    }
  };

  return (
    <aside className="vc-chat" aria-label={t('lounge.voiceChat.label', { name: channel.name })}>
      <header className="vc-chat-head">
        <Icon name="message" size={20} />
        <h2 className="vc-chat-title">{t('lounge.voiceChat.title')}</h2>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.voiceChat.close')}
          onClick={onClose}
        >
          <Icon name="close" size={20} />
        </button>
      </header>
      <MessageList
        channelId={channel.id}
        guildId={channel.guildId}
        title={channel.name}
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
        key={channel.id}
        ref={composer}
        channelId={channel.id}
        guildId={channel.guildId}
        target={`#${channel.name}`}
        slowmodeSeconds={channel.slowmodeSeconds}
        bypassSlowmode={
          channel.permissions
            ? channel.permissions.manageMessages || channel.permissions.manageChannels
            : !!(guild?.permissions.manageMessages || guild?.permissions.manageChannels)
        }
        canMentionEveryone={
          channel.permissions?.mentionEveryone ?? guild?.permissions.mentionEveryone ?? false
        }
        timedOutUntil={timedOutUntil}
        canTts={false}
        canStartThread={false}
        canSend={channel.canSend}
        replyTo={replyTo}
        onCancelReply={() => setReplyTo(null)}
        onEditLast={editLast}
      />
    </aside>
  );
}
