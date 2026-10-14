import { Icon, Tooltip, useToast } from '@sideroom/ui';
import { useT } from '../i18n';
import { useOpenUserSettings } from '../settings/user/nav';
import { useCurrentUser, usePresence } from './data';
import { useSelfPresence } from './shell/selfPresence';
import { statusLabel, StatusMenu } from './shell/StatusMenu';
import { voiceController } from '../voice/controller';
import { useVoiceSession } from '../voice/session';
import { VoicePanel } from './voice/VoicePanel';

export function UserPanel() {
  const t = useT();
  const me = useCurrentUser();
  const presence = usePresence(me.id);
  const selfStatus = useSelfPresence((s) => s.status);
  const selfCustom = useSelfPresence((s) => s.customText);
  const toast = useToast();
  const micMuted = useVoiceSession((s) => s.selfMute || s.serverMute);
  const serverMuted = useVoiceSession((s) => s.serverMute);
  const deafened = useVoiceSession((s) => s.selfDeaf || s.serverDeaf);
  const serverDeaf = useVoiceSession((s) => s.serverDeaf);
  const toggleMic = () => voiceController().toggleMute();
  const toggleDeafen = () => voiceController().toggleDeafen();
  const openSettings = useOpenUserSettings();

  const copy = () => {
    void navigator.clipboard?.writeText(me.username);
    toast.show({ title: t('lounge.userPanel.copied'), duration: 1500 });
  };

  return (
    <div className="lng-user-area">
      <VoicePanel />
      <div className="lng-user-panel">
        <StatusMenu name={me.displayName} />
        <button
          type="button"
          className="lng-user-id"
          onClick={copy}
          aria-label={t('lounge.userPanel.copyLabel', {
            name: me.displayName,
            status: statusLabel(selfStatus),
          })}
        >
          <span className="lng-user-text">
            <span className="lng-user-name">{me.displayName}</span>
            <span className="lng-user-sub">
              {selfCustom ??
                (presence.activity?.kind === 'custom'
                  ? presence.activity.text
                  : statusLabel(selfStatus))}
            </span>
          </span>
        </button>
        <Tooltip
          content={
            serverMuted
              ? t('lounge.userPanel.mutedByMod')
              : micMuted
                ? t('lounge.userPanel.unmute')
                : t('lounge.userPanel.mute')
          }
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={micMuted ? t('lounge.userPanel.unmute') : t('lounge.userPanel.mute')}
            aria-pressed={micMuted}
            disabled={serverMuted}
            onClick={toggleMic}
          >
            <Icon name={micMuted ? 'mic-off' : 'mic'} size={20} />
          </button>
        </Tooltip>
        <Tooltip
          content={
            serverDeaf
              ? t('lounge.userPanel.deafenedByMod')
              : deafened
                ? t('lounge.userPanel.undeafen')
                : t('lounge.userPanel.deafen')
          }
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={deafened ? t('lounge.userPanel.undeafen') : t('lounge.userPanel.deafen')}
            aria-pressed={deafened}
            disabled={serverDeaf}
            onClick={toggleDeafen}
          >
            <Icon name={deafened ? 'volume-off' : 'headphones'} size={20} />
          </button>
        </Tooltip>
        <Tooltip content={t('lounge.userPanel.settings')}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.userPanel.settings')}
            onClick={() => openSettings()}
          >
            <Icon name="settings" size={20} />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
