import { Avatar, Icon, Tooltip, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { ComingSoon } from './components/ComingSoon';
import { useCurrentUser, usePresence } from './data';
import { useLoungeUi } from './store';

export function UserPanel() {
  const me = useCurrentUser();
  const presence = usePresence(me.id);
  const toast = useToast();
  const voice = useLoungeUi((s) => s.voice);
  const joinVoice = useLoungeUi((s) => s.joinVoice);
  const micMuted = useLoungeUi((s) => s.micMuted);
  const deafened = useLoungeUi((s) => s.deafened);
  const toggleMic = useLoungeUi((s) => s.toggleMic);
  const toggleDeafen = useLoungeUi((s) => s.toggleDeafen);
  const [settings, setSettings] = useState(false);

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
        <button
          type="button"
          className="lng-user-id"
          onClick={copy}
          aria-label={`${me.displayName}, ${presence.status}. Click to copy username`}
        >
          <Avatar name={me.displayName} size={32} status={presence.status} />
          <span className="lng-user-text">
            <span className="lng-user-name">{me.displayName}</span>
            <span className="lng-user-sub">
              {presence.activity?.kind === 'custom'
                ? presence.activity.text
                : presence.status === 'online'
                  ? 'Online'
                  : presence.status}
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
            onClick={() => setSettings(true)}
          >
            <Icon name="settings" size={20} />
          </button>
        </Tooltip>
      </div>
      <ComingSoon open={settings} onClose={() => setSettings(false)} title="User Settings">
        <p>User Settings is built under the settings inventory items.</p>
      </ComingSoon>
    </div>
  );
}
