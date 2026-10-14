import { Link } from '@tanstack/react-router';
import { Button, Icon, Notice, Tooltip } from '@sideroom/ui';
import { useT, type MessageKey } from '../../i18n';
import { useOpenUserSettings } from '../../settings/user/nav';
import { voiceController } from '../../voice/controller';
import { isInVoice, useVoiceSession, type SessionStatus } from '../../voice/session';
import { QUALITY_LABEL } from '../../voice/quality';
import { useGuild } from '../data';
import { QualityBars } from './QualityBars';

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
 * with a ping tooltip, settings and disconnect. It also shows why a join failed, with the fix for a blocked
 * or missing microphone.
 */
export function VoicePanel() {
  const t = useT();
  const s = useVoiceSession();
  const guild = useGuild(s.guildId ?? undefined);
  const openSettings = useOpenUserSettings();
  const controller = voiceController();

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
        <Tooltip content={pingLabel}>
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
      {s.pttForced && <p className="vc-note">{t('lounge.voicePanel.pttRequired')}</p>}
    </div>
  );
}
