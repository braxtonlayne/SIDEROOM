import { useState } from 'react';
import { Link } from '@tanstack/react-router';
import { Button, Icon, Notice, Tooltip } from '@sideroom/ui';
import { useT, type MessageKey } from '../../i18n';
import { useOpenUserSettings } from '../../settings/user/nav';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { useVideoLimits } from '../../voice/limits';
import { isInVoice, useVoiceSession, type SessionStatus } from '../../voice/session';
import { QUALITY_LABEL } from '../../voice/quality';
import { useLocalVideo } from '../../voice/video';
import { useGuild } from '../data';
import { PingGraph } from './PingGraph';
import { QualityBars } from './QualityBars';
import { ScreenSharePicker } from './ScreenSharePicker';
import { SoundboardButton } from './SoundboardPopover';
import './voice-p10.css';

const STATUS_TEXT: Record<SessionStatus, MessageKey | ''> = {
  idle: '',
  error: '',
  joining: 'lounge.voicePanel.rtcConnecting',
  connecting: 'lounge.voicePanel.rtcConnecting',
  connected: 'lounge.voicePanel.connected',
  reconnecting: 'lounge.voicePanel.reconnecting',
};

/**
 * The connection panel above the user panel (VOX-UI-001): state, channel and server link, connection quality
 * with a ping tooltip and graph, camera and screen share buttons, settings and disconnect. It also shows why a
 * join failed, with the fix for a blocked or missing microphone.
 */
export function VoicePanel() {
  const t = useT();
  const s = useVoiceSession();
  const guild = useGuild(s.guildId ?? undefined);
  const openSettings = useOpenUserSettings();
  const limits = useVideoLimits(s.guildId);
  const screen = useLocalVideo((st) => st.screen);
  const controller = voiceController();
  const [picker, setPicker] = useState(false);

  if (s.status === 'error' && s.error) {
    const mic = s.error.code === 'mic';
    return (
      <div className="vc-panel" data-testid="voice-panel">
        <Notice
          variant="danger"
          title={mic ? t('lounge.voicePanel.micError') : t('lounge.voicePanel.joinError')}
          role="alert"
          onDismiss={() => controller.dismissError()}
        >
          {s.error.message}
          <div className="vc-error-actions">
            {mic && (
              <Button size="sm" variant="secondary" onClick={() => openSettings('voice')}>
                {t('lounge.voicePanel.voiceSettings')}
              </Button>
            )}
            {s.error.code !== 'disconnected' && (
              <Button size="sm" variant="secondary" onClick={() => controller.retry()}>
                {t('lounge.voicePanel.tryAgain')}
              </Button>
            )}
          </div>
        </Notice>
      </div>
    );
  }
  if (!isInVoice(s) || !s.channelId || !s.guildId) return null;

  const connected = s.status === 'connected';
  const pingLabel =
    s.quality.pingMs !== null
      ? t('lounge.voicePanel.pingLabel', {
          label: QUALITY_LABEL[s.quality.level],
          ms: s.quality.pingMs,
        })
      : QUALITY_LABEL[s.quality.level];
  return (
    <div
      className="vc-panel"
      role="region"
      aria-label={t('lounge.voicePanel.region')}
      data-testid="voice-panel"
    >
      <div className="vc-panel-row">
        <Tooltip
          content={
            <span className="vc-ping-tip">
              <span>{pingLabel}</span>
              <PingGraph history={s.pingHistory} />
            </span>
          }
        >
          <button type="button" className="vc-quality-btn" aria-label={pingLabel}>
            <QualityBars quality={connected ? s.quality : { ...s.quality, level: 'unknown' }} />
          </button>
        </Tooltip>
        <div className="vc-panel-text">
          <div className="vc-state" data-ok={connected || undefined} role="status">
            {STATUS_TEXT[s.status] ? t(STATUS_TEXT[s.status] as MessageKey) : ''}
          </div>
          <Link
            to="/channels/$guildId/$channelId"
            params={{ guildId: s.guildId, channelId: s.channelId }}
            className="vc-where"
          >
            {s.channelName}
            {guild ? t('lounge.voicePanel.inGuild', { name: guild.name }) : ''}
          </Link>
        </div>
        <Tooltip content={t('lounge.voicePanel.voiceSettings')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.voicePanel.voiceSettings')}
            onClick={() => openSettings('voice')}
          >
            <Icon name="settings" size={20} />
          </button>
        </Tooltip>
        <Tooltip content={t('lounge.voicePanel.disconnect')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.voicePanel.disconnect')}
            onClick={() => controller.leave()}
          >
            <Icon name="logout" size={20} />
          </button>
        </Tooltip>
      </div>
      <div className="vc-panel-actions">
        <Tooltip
          content={s.camera ? t('lounge.voicePanel.cameraOff') : t('lounge.voicePanel.cameraOn')}
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={
              s.camera ? t('lounge.voicePanel.cameraOff') : t('lounge.voicePanel.cameraOn')
            }
            aria-pressed={s.camera}
            data-active={s.camera || undefined}
            onClick={() =>
              s.camera ? controller.stopCamera() : void controller.startCamera(limits.profiles)
            }
          >
            <Icon name={s.camera ? 'video' : 'video-off'} size={20} />
          </button>
        </Tooltip>
        <Tooltip
          content={s.screen ? t('lounge.voicePanel.stopShare') : t('lounge.voicePanel.share')}
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={s.screen ? t('lounge.voicePanel.stopShare') : t('lounge.voicePanel.share')}
            aria-pressed={s.screen}
            data-active={s.screen || undefined}
            onClick={() => (s.screen ? controller.stopScreenShare() : setPicker(true))}
          >
            <Icon name="screen-share" size={20} />
          </button>
        </Tooltip>
        {s.guildId && s.channelId && s.guildId !== DM_GUILD_ID && (
          <SoundboardButton guildId={s.guildId} channelId={s.channelId} compact />
        )}
      </div>
      {s.notice && (
        <Notice variant="info" role="status" onDismiss={() => controller.dismissNotice()}>
          {s.notice}
        </Notice>
      )}
      {s.reducedQuality && (
        <Notice variant="warning" role="status" data-testid="governor-notice">
          {t('lounge.voiceNotices.reduced')}
        </Notice>
      )}
      {s.refusal && (
        <Notice
          variant="warning"
          role="alert"
          data-testid="refusal-notice"
          onDismiss={() => controller.dismissRefusal()}
        >
          {s.refusal.message}
          <div className="vc-error-actions">
            <Link to="/support" className="vc-link">
              {t('lounge.voiceNotices.support')}
            </Link>
            {s.refusal.code === 'videoLimit' && (
              <Link to="/support" className="vc-link">
                {t('lounge.voiceNotices.patron')}
              </Link>
            )}
          </div>
        </Notice>
      )}
      {s.pttForced && <p className="vc-note">{t('lounge.voicePanel.pttRequired')}</p>}
      {picker && (
        <ScreenSharePicker
          open
          onClose={() => setPicker(false)}
          profiles={limits.profiles}
          onStart={(opts) => void controller.startScreenShare(opts)}
          initial={
            screen
              ? {
                  source: screen.source,
                  audio: screen.audio,
                  hint: screen.hint,
                  presetId: screen.preset.id,
                }
              : undefined
          }
        />
      )}
    </div>
  );
}
