import { Link } from '@tanstack/react-router';
import { Button, Icon, Notice, Tooltip } from '@sideroom/ui';
import { useOpenUserSettings } from '../../settings/user/nav';
import { voiceController } from '../../voice/controller';
import { isInVoice, useVoiceSession, type SessionStatus } from '../../voice/session';
import { QUALITY_LABEL } from '../../voice/quality';
import { useGuild } from '../data';
import { QualityBars } from './QualityBars';

const STATUS_TEXT: Record<SessionStatus, string> = {
  idle: '',
  error: '',
  joining: 'RTC Connecting',
  connecting: 'RTC Connecting',
  connected: 'Voice Connected',
  reconnecting: 'Reconnecting',
};

/**
 * The connection panel above the user panel (VOX-UI-001): state, channel and server link, connection quality
 * with a ping tooltip, settings and disconnect. It also shows why a join failed, with the fix for a blocked
 * or missing microphone.
 */
export function VoicePanel() {
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
          title={mic ? "Can't use your microphone" : "Couldn't join voice"}
          role="alert"
          onDismiss={() => controller.dismissError()}
        >
          {s.error.message}
          <div className="vc-error-actions">
            {mic && (
              <Button size="sm" variant="secondary" onClick={() => openSettings('voice')}>
                Voice settings
              </Button>
            )}
            {s.error.code !== 'disconnected' && (
              <Button size="sm" variant="secondary" onClick={() => controller.retry()}>
                Try again
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
    QUALITY_LABEL[s.quality.level] + (s.quality.pingMs !== null ? ` (${s.quality.pingMs} ms)` : '');
  return (
    <div className="vc-panel" role="region" aria-label="Voice connection" data-testid="voice-panel">
      <div className="vc-panel-row">
        <Tooltip content={pingLabel}>
          <button type="button" className="vc-quality-btn" aria-label={pingLabel}>
            <QualityBars quality={connected ? s.quality : { ...s.quality, level: 'unknown' }} />
          </button>
        </Tooltip>
        <div className="vc-panel-text">
          <div className="vc-state" data-ok={connected || undefined} role="status">
            {STATUS_TEXT[s.status]}
          </div>
          <Link
            to="/channels/$guildId/$channelId"
            params={{ guildId: s.guildId, channelId: s.channelId }}
            className="vc-where"
          >
            {s.channelName}
            {guild ? ` / ${guild.name}` : ''}
          </Link>
        </div>
        <Tooltip content="Voice settings">
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Voice settings"
            onClick={() => openSettings('voice')}
          >
            <Icon name="settings" size={20} />
          </button>
        </Tooltip>
        <Tooltip content="Disconnect">
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="Disconnect"
            onClick={() => controller.leave()}
          >
            <Icon name="logout" size={20} />
          </button>
        </Tooltip>
      </div>
      {s.pttForced && <p className="vc-note">This channel requires push-to-talk.</p>}
    </div>
  );
}
