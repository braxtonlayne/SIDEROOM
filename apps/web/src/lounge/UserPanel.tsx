import { Icon, Tooltip, useToast } from '@sideroom/ui';
import { useOpenUserSettings } from '../settings/user/nav';
import { useCurrentUser, usePresence } from './data';
import { useSelfPresence } from './shell/selfPresence';
import { statusLabel, StatusMenu } from './shell/StatusMenu';
import { useLoungeUi } from './store';

export function UserPanel() {
  const me = useCurrentUser();
  const presence = usePresence(me.id);
  const selfStatus = useSelfPresence((s) => s.status);
  const selfCustom = useSelfPresence((s) => s.customText);
  const toast = useToast();
  const voice = useLoungeUi((s) => s.voice);
  const joinVoice = useLoungeUi((s) => s.joinVoice);
  const micMuted = useLoungeUi((s) => s.micMuted);
  const deafened = useLoungeUi((s) => s.deafened);
  const toggleMic = useLoungeUi((s) => s.toggleMic);
  const toggleDeafen = useLoungeUi((s) => s.toggleDeafen);
  const openSettings = useOpenUserSettings();

  const copy = () => {
    void navigator.clipboard?.writeText(me.username);
    toast.show({ title: 'Copied!', duration: 1500 });
  };

  return (
    <div className="lng-user-area">
      {voice && (
        <div className="lng-voice-panel" role="status" aria-label="Voice connection">
          <div>
            <div className="lng-voice-state">Voice Connected</div>
            <div className="lng-voice-where">{voice.name}</div>
          </div>
          <Tooltip content="Disconnect">
            <button
              type="button"
              className="lng-icon-btn"
              aria-label="Disconnect"
              onClick={() => joinVoice(null)}
            >
              <Icon name="logout" size={20} />
            </button>
          </Tooltip>
        </div>
      )}
      <div className="lng-user-panel">
        <StatusMenu name={me.displayName} />
        <button
          type="button"
          className="lng-user-id"
          onClick={copy}
          aria-label={`${me.displayName}, ${statusLabel(selfStatus)}. Click to copy username`}
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
        <Tooltip content={micMuted ? 'Unmute' : 'Mute'}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={micMuted ? 'Unmute' : 'Mute'}
            aria-pressed={micMuted}
            onClick={toggleMic}
          >
            <Icon name={micMuted ? 'mic-off' : 'mic'} size={20} />
          </button>
        </Tooltip>
        <Tooltip content={deafened ? 'Undeafen' : 'Deafen'}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={deafened ? 'Undeafen' : 'Deafen'}
            aria-pressed={deafened}
            onClick={toggleDeafen}
          >
            <Icon name={deafened ? 'volume-off' : 'headphones'} size={20} />
          </button>
        </Tooltip>
        <Tooltip content="User Settings">
          <button
            type="button"
            className="lng-icon-btn"
            aria-label="User Settings"
            onClick={() => openSettings()}
          >
            <Icon name="settings" size={20} />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
