import { useRef } from 'react';
import { Avatar, Button, ContextMenu, Icon, Popover, Slider } from '@sideroom/ui';
import { t, useT } from '../../i18n';
import { voiceController } from '../../voice/controller';
import { useE2ee } from '../../voice/e2ee/store';
import { useVoiceSession } from '../../voice/session';
import { useVoiceSettings, userVolumeOf } from '../../voice/settings';
import { streamKey, useStreams } from '../../voice/streams';
import { presetLabel, sourceKey, useLocalVideo, useRemoteVideo } from '../../voice/video';
import { useCurrentUser, useUsers, type VoiceMember } from '../data';
import { QualityBars } from './QualityBars';
import { useStreamPreview } from './useStreamPreview';
import { useTileViewport } from './useTileViewport';
import { useVoiceUserMenu } from './useVoiceUserMenu';
import { VideoSurface } from './VideoSurface';
import './voice-p10.css';
import './voice-p13.css';

export function stateLabel(p: VoiceMember, name: string, self: boolean): string {
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
    p.video ? t('lounge.voiceChannel.stateCamera') : '',
    p.streaming ? t('lounge.voiceChannel.stateLive') : '',
  ].filter(Boolean);
  return `${name}${self ? t('lounge.voiceChannel.you') : ''}${bits.length ? `, ${bits.join(', ')}` : ''}`;
}

export function VolumeControl({ userId, name }: { userId: string; name: string }) {
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

export interface TileProps {
  guildId: string;
  channelId: string;
  participant: VoiceMember;
  onError: (m: string) => void;
  /** This tile is the spotlight in focus mode. */
  focused?: boolean;
  /** Click to spotlight (focus mode) or release it. */
  onToggleFocus?: (() => void) | undefined;
  /** Extra buttons in the tile's action row (a stage's "Move to audience"). */
  actions?: React.ReactNode;
}

/** One person: their camera when it is on, else their avatar; name, state icons, speaking ring (VOX-UI-003). */
export function ParticipantTile({
  guildId,
  channelId,
  participant: p,
  onError,
  focused,
  onToggleFocus,
  actions,
}: TileProps) {
  const t = useT();
  const users = useUsers();
  const me = useCurrentUser();
  const quality = useVoiceSession((s) => s.quality);
  const sessionCamera = useVoiceSession((s) => s.camera);
  const localCamera = useLocalVideo((s) => s.camera);
  const cameraOff = useRemoteVideo((s) => !!s.videoOff[p.userId]);
  const remoteCamera = useRemoteVideo((s) =>
    s.videoOff[p.userId] ? undefined : s.camera[p.userId],
  );
  // End-to-end encrypted with this person in the current epoch (VOX-UI-003, VOX-E2E-007).
  const e2eeSecure = useE2ee(
    (s) =>
      s.active && s.call?.status === 'secure' && s.call.members.some((m) => m.userId === p.userId),
  );
  const u = users[p.userId];
  const name = u?.displayName ?? t('lounge.voiceChannel.unknown');
  const self = p.userId === me.id;
  const stream = self ? (sessionCamera ? localCamera?.stream : undefined) : remoteCamera;
  const hasVideo = !!stream;
  const tileRef = useRef<HTMLDivElement | null>(null);
  useTileViewport(tileRef, p.userId, 'camera', !self && !!remoteCamera);
  const menu = useVoiceUserMenu(guildId, { id: p.userId, name }, onError, channelId);
  const label = stateLabel({ ...p, video: p.video || hasVideo }, name, self);
  return (
    <li className="vc-tile-item" data-focused={focused || undefined}>
      <ContextMenu
        items={menu}
        label={t('lounge.voiceChannel.voiceActions', { name })}
        disabled={self}
      >
        <div
          ref={tileRef}
          className="vc-tile"
          data-speaking={p.speaking || undefined}
          data-video={hasVideo || undefined}
          data-testid={`voice-tile-${p.userId}`}
          role="group"
          aria-label={label}
        >
          {stream ? (
            <VideoSurface
              stream={stream}
              mirror={self}
              label={t('lounge.voiceChannel.cameraOf', { name })}
            />
          ) : (
            <Avatar name={name} src={u?.avatarUrl ?? null} size={80} speaking={p.speaking} />
          )}
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
            {e2eeSecure && (
              <Icon name="shield" size={16} label={t('lounge.voiceChannel.e2eeSecure')} />
            )}
            {p.video && !hasVideo && !cameraOff && (
              <Icon name="video" size={16} label={t('lounge.voiceChannel.cameraOn')} />
            )}
            {cameraOff && (
              <Icon name="video-off" size={16} label={t('lounge.voiceChannel.videoDisabled')} />
            )}
          </div>
          {self && (
            <span className="vc-tile-quality" data-testid="tile-quality">
              <QualityBars quality={quality} />
            </span>
          )}
          <div className="vc-tile-actions">
            {onToggleFocus && (
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={
                  focused
                    ? t('lounge.voiceChannel.unfocus', { name })
                    : t('lounge.voiceChannel.focus', { name })
                }
                aria-pressed={!!focused}
                onClick={onToggleFocus}
              >
                <Icon name={focused ? 'minimize' : 'maximize'} size={16} />
              </button>
            )}
            {!self && <VolumeControl userId={p.userId} name={name} />}
            {actions}
          </div>
        </div>
      </ContextMenu>
    </li>
  );
}

/**
 * A screen share (Go Live) tile (VOX-UI-004): the viewer's own share shows live with a Stop button; someone
 * else's shows a LIVE badge, "Watch Stream" and, once watched, the video with its viewer controls (volume,
 * full screen, quality info).
 */
export function StreamTile({
  guildId,
  channelId,
  userId,
  name,
  onError,
}: {
  guildId: string;
  channelId: string;
  userId: string;
  name: string;
  onError?: (m: string) => void;
}) {
  const t = useT();
  const me = useCurrentUser();
  const self = userId === me.id;
  const localScreen = useLocalVideo((s) => s.screen);
  const sessionScreen = useVoiceSession((s) => s.screen);
  const remote = useRemoteVideo((s) => s.screen[userId]);
  const watching = useRemoteVideo((s) => !!s.watching[userId]);
  const published = useRemoteVideo((s) => s.info[sourceKey(userId, 'screen')]);
  const layer = useRemoteVideo((s) => s.layers[sourceKey(userId, 'screen')]);
  const volume = useVoiceSettings((s) => userVolumeOf(s.settings, userId));
  const audience = useStreams((s) => s.byOwner[userId]);
  const key = streamKey(guildId, channelId, userId);
  const tileRef = useRef<HTMLDivElement | null>(null);
  useTileViewport(tileRef, userId, 'screen', !self && watching);
  const preview = useStreamPreview(key, !self && !watching);
  const stream = self
    ? sessionScreen
      ? localScreen?.stream
      : undefined
    : watching
      ? remote
      : undefined;
  const info =
    self && localScreen
      ? presetLabel(localScreen.preset)
      : published && published.height
        ? `${published.height}p ${published.fps} fps${layer ? ` (${layer})` : ''}`
        : null;
  const viewerCount = audience?.viewerIds.length ?? 0;
  const fullscreen = (el: HTMLElement | null) => {
    if (!el?.requestFullscreen) return onError?.(t('lounge.voiceStream.noFullscreen'));
    void el.requestFullscreen().catch(() => onError?.(t('lounge.voiceStream.noFullscreen')));
  };
  return (
    <li className="vc-tile-item vc-stream-item">
      <div
        ref={tileRef}
        className="vc-tile vc-stream"
        data-testid={`stream-tile-${userId}`}
        role="group"
        aria-label={self ? t('lounge.voiceStream.yours') : t('lounge.voiceStream.of', { name })}
      >
        <span className="vc-live-badge">{t('lounge.voiceStream.live')}</span>
        {(self || watching) && audience && (
          <span className="vc-viewer-count" data-testid={`stream-viewers-${userId}`}>
            <Icon name="users" size={16} />
            {t('lounge.voiceStream.viewers', { n: viewerCount })}
          </span>
        )}
        {stream ? (
          <VideoSurface
            stream={stream}
            muted={self}
            volume={self ? undefined : Math.min(1, volume / 100)}
            label={self ? t('lounge.voiceStream.yours') : t('lounge.voiceStream.of', { name })}
          />
        ) : (
          <div className="vc-stream-idle" data-preview={preview ? 'true' : undefined}>
            {preview && (
              <img
                className="vc-stream-preview"
                src={preview}
                alt={t('lounge.voiceStream.previewOf', { name })}
              />
            )}
            <Icon name="screen-share" size={24} />
            <span>{t('lounge.voiceStream.of', { name })}</span>
            {!self && (
              <Button size="sm" onClick={() => voiceController().watchStream(userId)}>
                {t('lounge.voiceStream.watch')}
              </Button>
            )}
            {!self && watching && !remote && (
              <span className="vc-note" role="status">
                {t('lounge.voiceStream.notAvailable')}
              </span>
            )}
          </div>
        )}
        <div className="vc-stream-bar">
          {info && (
            <span className="vc-stream-info">
              {t('lounge.voiceStream.quality', { quality: info })}
            </span>
          )}
          {self ? (
            <Button size="sm" variant="danger" onClick={() => voiceController().stopScreenShare()}>
              {t('lounge.voiceStream.stop')}
            </Button>
          ) : (
            <>
              {watching && <VolumeControl userId={userId} name={name} />}
              {watching && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => voiceController().unwatchStream(userId)}
                >
                  {t('lounge.voiceStream.stopWatching')}
                </Button>
              )}
            </>
          )}
          {stream && (
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={t('lounge.voiceStream.fullscreen')}
              onClick={(e) => fullscreen(e.currentTarget.closest<HTMLElement>('.vc-stream'))}
            >
              <Icon name="maximize" size={16} />
            </button>
          )}
        </div>
      </div>
    </li>
  );
}
