import { useState } from 'react';
import { Avatar, Button, ContextMenu, EmptyState, Icon, Popover, Slider } from '@sideroom/ui';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { useVoiceSettings, userVolumeOf } from '../../voice/settings';
import { ChannelHeader } from '../ChannelHeader';
import { CHANNEL_ICON } from '../ChannelSidebar';
import { useCurrentUser, useUsers, useVoiceMembers, type Channel, type VoiceMember } from '../data';
import { useJoinVoice } from './actions';
import { useVoicePerms } from './perms';
import { useVoiceMenuEntries } from './useVoiceMenu';

function stateLabel(p: VoiceMember, name: string, self: boolean): string {
  const bits = [
    p.serverMuted ? 'muted by a moderator' : p.muted ? 'muted' : '',
    p.serverDeafened ? 'deafened by a moderator' : p.deafened ? 'deafened' : '',
    p.speaking ? 'speaking' : '',
  ].filter(Boolean);
  return `${name}${self ? ' (you)' : ''}${bits.length ? `, ${bits.join(', ')}` : ''}`;
}

function VolumeControl({ userId, name }: { userId: string; name: string }) {
  const volume = useVoiceSettings((s) => userVolumeOf(s.settings, userId));
  const setVolume = useVoiceSettings((s) => s.setUserVolume);
  return (
    <Popover
      label={`${name} volume`}
      placement="bottom"
      trigger={
        <button type="button" className="lng-icon-btn" aria-label={`Volume for ${name}`}>
          <Icon name="volume" size={16} />
        </button>
      }
    >
      <div className="vc-volume">
        <Slider
          label={`${name} volume`}
          min={0}
          max={200}
          step={5}
          value={volume}
          showValue
          formatValue={(v) => `${v}%`}
          onValueChange={(v) => setVolume(userId, v)}
          marks={[{ value: 100, label: '100%' }]}
        />
      </div>
    </Popover>
  );
}

function Tile({
  guildId,
  channelId,
  participant: p,
  onError,
}: {
  guildId: string;
  channelId: string;
  participant: VoiceMember;
  onError: (m: string) => void;
}) {
  const users = useUsers();
  const me = useCurrentUser();
  const u = users[p.userId];
  const name = u?.displayName ?? 'Unknown';
  const self = p.userId === me.id;
  const menu = useVoiceMenuEntries(guildId, { id: p.userId, name }, onError, channelId);
  return (
    <li className="vc-tile-item">
      <ContextMenu items={menu} label={`${name} voice actions`} disabled={self}>
        <div
          className="vc-tile"
          data-speaking={p.speaking || undefined}
          role="group"
          aria-label={stateLabel(p, name, self)}
        >
          <Avatar name={name} src={u?.avatarUrl ?? null} size={80} speaking={p.speaking} />
          <div className="vc-tile-name">
            <span>{name}</span>
            {p.muted && (
              <Icon
                name="mic-off"
                size={16}
                label={p.serverMuted ? 'Muted by a moderator' : 'Muted'}
              />
            )}
            {p.deafened && (
              <Icon
                name="volume-off"
                size={16}
                label={p.serverDeafened ? 'Deafened by a moderator' : 'Deafened'}
              />
            )}
          </div>
          {!self && (
            <div className="vc-tile-actions">
              <VolumeControl userId={p.userId} name={name} />
            </div>
          )}
        </div>
      </ContextMenu>
    </li>
  );
}

/**
 * The view of a server voice channel: join screen, participant tiles (avatars with speaking rings; video and
 * screen share come later) and the call controls (VOX-UI-002/003/005).
 */
export function VoiceChannelView({
  channel,
  onBack,
}: {
  channel: Channel;
  onBack?: (() => void) | undefined;
}) {
  const join = useJoinVoice();
  const session = useVoiceSession();
  const participants = useVoiceMembers(channel.id);
  const perms = useVoicePerms(channel.guildId, channel.id);
  const here =
    session.channelId === channel.id && session.status !== 'idle' && session.status !== 'error';
  const full = !!channel.userLimit && participants.length >= channel.userLimit && !here;
  const controller = voiceController();
  const mutedNow = session.selfMute || session.serverMute;
  const deafNow = session.selfDeaf || session.serverDeaf;
  const [error, setError] = useState<string | null>(null);

  return (
    <section className="lng-channel-view vc-view" aria-label={channel.name}>
      <ChannelHeader
        icon={CHANNEL_ICON[channel.type]}
        title={channel.name}
        isDm={false}
        status={undefined}
        topic={channel.topic}
        membersOpen={false}
        onToggleMembers={() => undefined}
        onBack={onBack}
        searchLabel={`Search ${channel.name}`}
        guildId={channel.guildId}
        channelId={channel.id}
      />
      <div className="vc-stage">
        {error && (
          <p role="alert" className="vc-inline-error">
            {error}
          </p>
        )}
        {!here ? (
          <div className="vc-join">
            <EmptyState
              title={channel.name}
              description={
                participants.length
                  ? `${participants.length} ${participants.length === 1 ? 'person is' : 'people are'} in this voice channel.`
                  : 'Nobody is in this voice channel yet.'
              }
              action={
                <Button onClick={() => join(channel)} disabled={full || perms.connect === false}>
                  {full ? 'Channel full' : 'Join Voice'}
                </Button>
              }
            />
            {!perms.useVad && <p className="vc-note">This channel requires push-to-talk.</p>}
            {participants.length > 0 && (
              <ul className="vc-grid" aria-label="In this channel">
                {participants.map((p) => (
                  <Tile
                    key={p.userId}
                    guildId={channel.guildId}
                    channelId={channel.id}
                    participant={p}
                    onError={setError}
                  />
                ))}
              </ul>
            )}
          </div>
        ) : (
          <ul className="vc-grid" aria-label={`In ${channel.name}`}>
            {participants.map((p) => (
              <Tile
                key={p.userId}
                guildId={channel.guildId}
                channelId={channel.id}
                participant={p}
                onError={setError}
              />
            ))}
          </ul>
        )}
      </div>
      {here && (
        <div className="vc-controls" role="toolbar" aria-label="Call controls">
          <button
            type="button"
            className="vc-ctl"
            aria-label={mutedNow ? 'Unmute' : 'Mute'}
            aria-pressed={mutedNow}
            disabled={session.serverMute}
            data-on={mutedNow || undefined}
            onClick={() => controller.toggleMute()}
          >
            <Icon name={mutedNow ? 'mic-off' : 'mic'} size={24} />
          </button>
          <button
            type="button"
            className="vc-ctl"
            aria-label={deafNow ? 'Undeafen' : 'Deafen'}
            aria-pressed={deafNow}
            disabled={session.serverDeaf}
            data-on={deafNow || undefined}
            onClick={() => controller.toggleDeafen()}
          >
            <Icon name={deafNow ? 'volume-off' : 'headphones'} size={24} />
          </button>
          <button
            type="button"
            className="vc-ctl vc-ctl-danger"
            aria-label="Disconnect"
            onClick={() => controller.leave()}
          >
            <Icon name="logout" size={24} />
          </button>
        </div>
      )}
    </section>
  );
}
