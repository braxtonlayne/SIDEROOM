import { useState } from 'react';
import { Avatar, Button, ContextMenu, EmptyState, Icon, Popover, Slider } from '@sideroom/ui';
import { t, useT } from '../../i18n';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { useVoiceSettings, userVolumeOf } from '../../voice/settings';
import { ChannelHeader } from '../ChannelHeader';
import { CHANNEL_ICON } from '../ChannelSidebar';
import { useCurrentUser, useUsers, useVoiceMembers, type Channel, type VoiceMember } from '../data';
import { useJoinVoice } from './actions';
import { useVoicePerms } from './perms';
import { StartPartyButton } from '../../watchparty/StartPartyButton';
import { WatchPartyStrip } from '../../watchparty/WatchPartyStrip';
import { useVoiceMenuEntries } from './useVoiceMenu';

function stateLabel(p: VoiceMember, name: string, self: boolean): string {
  const bits = [
    p.serverMuted
      ? t('lounge.voiceChannel.stateMutedByMod')
      : p.muted
        ? t('lounge.voiceChannel.stateMuted')
        : '',
    p.serverDeafened
      ? t('lounge.voiceChannel.stateDeafenedByMod')
      : p.deafened
        ? t('lounge.voiceChannel.stateDeafened')
        : '',
    p.speaking ? t('lounge.voiceChannel.stateSpeaking') : '',
  ].filter(Boolean);
  return `${name}${self ? t('lounge.voiceChannel.you') : ''}${bits.length ? `, ${bits.join(', ')}` : ''}`;
}

function VolumeControl({ userId, name }: { userId: string; name: string }) {
  const t = useT();
  const volume = useVoiceSettings((s) => userVolumeOf(s.settings, userId));
  const setVolume = useVoiceSettings((s) => s.setUserVolume);
  return (
    <Popover
      label={t('lounge.voiceChannel.volumeOf', { name })}
      placement="bottom"
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.voiceChannel.volumeFor', { name })}
        >
          <Icon name="volume" size={16} />
        </button>
      }
    >
      <div className="vc-volume">
        <Slider
          label={t('lounge.voiceChannel.volumeOf', { name })}
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
  const t = useT();
  const users = useUsers();
  const me = useCurrentUser();
  const u = users[p.userId];
  const name = u?.displayName ?? t('lounge.voiceChannel.unknown');
  const self = p.userId === me.id;
  const menu = useVoiceMenuEntries(guildId, { id: p.userId, name }, onError, channelId);
  return (
    <li className="vc-tile-item">
      <ContextMenu
        items={menu}
        label={t('lounge.voiceChannel.voiceActions', { name })}
        disabled={self}
      >
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
                label={
                  p.serverMuted
                    ? t('lounge.voiceChannel.mutedByMod')
                    : t('lounge.voiceChannel.muted')
                }
              />
            )}
            {p.deafened && (
              <Icon
                name="volume-off"
                size={16}
                label={
                  p.serverDeafened
                    ? t('lounge.voiceChannel.deafenedByMod')
                    : t('lounge.voiceChannel.deafened')
                }
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
  const t = useT();
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
        searchLabel={t('lounge.voiceChannel.search', { name: channel.name })}
        guildId={channel.guildId}
        channelId={channel.id}
      />
      <WatchPartyStrip channelId={channel.id} />
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
                  ? t('lounge.voiceChannel.peopleHere', { n: participants.length })
                  : t('lounge.voiceChannel.nobodyHere')
              }
              action={
                <Button onClick={() => join(channel)} disabled={full || perms.connect === false}>
                  {full ? t('lounge.voiceChannel.full') : t('lounge.voiceChannel.join')}
                </Button>
              }
            />
            {!perms.useVad && <p className="vc-note">{t('lounge.voiceChannel.pttRequired')}</p>}
            {participants.length > 0 && (
              <ul className="vc-grid" aria-label={t('lounge.voiceChannel.inThisChannel')}>
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
          <ul
            className="vc-grid"
            aria-label={t('lounge.voiceChannel.inChannel', { name: channel.name })}
          >
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
        <div
          className="vc-controls"
          role="toolbar"
          aria-label={t('lounge.voiceChannel.callControls')}
        >
          <button
            type="button"
            className="vc-ctl"
            aria-label={mutedNow ? t('lounge.voiceChannel.unmute') : t('lounge.voiceChannel.mute')}
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
            aria-label={
              deafNow ? t('lounge.voiceChannel.undeafen') : t('lounge.voiceChannel.deafen')
            }
            aria-pressed={deafNow}
            disabled={session.serverDeaf}
            data-on={deafNow || undefined}
            onClick={() => controller.toggleDeafen()}
          >
            <Icon name={deafNow ? 'volume-off' : 'headphones'} size={24} />
          </button>
          <StartPartyButton channelId={channel.id} />
          <button
            type="button"
            className="vc-ctl vc-ctl-danger"
            aria-label={t('lounge.voiceChannel.disconnect')}
            onClick={() => controller.leave()}
          >
            <Icon name="logout" size={24} />
          </button>
        </div>
      )}
    </section>
  );
}
