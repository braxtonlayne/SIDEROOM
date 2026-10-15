import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Avatar, Icon } from '@sideroom/ui';
import { useT } from '../../i18n';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { useLocalVideo } from '../../voice/video';
import { useUsers, useVoiceMembers } from '../data';
import { VideoSurface } from './VideoSurface';
import './voice-p10.css';

/** The Document Picture-in-Picture API (Chromium); absent elsewhere. */
interface DocumentPip {
  requestWindow(opts?: { width?: number; height?: number }): Promise<Window>;
}
const pipApi = (): DocumentPip | undefined =>
  (globalThis as unknown as { documentPictureInPicture?: DocumentPip }).documentPictureInPicture;

export const isPopOutSupported = (): boolean => !!pipApi();

/** Copy the page's styles and theme attributes into the pop-out document, so the same CSS applies. */
export function copyStyles(from: Document, to: Document): void {
  for (const attr of Array.from(from.documentElement.attributes))
    to.documentElement.setAttribute(attr.name, attr.value);
  for (const node of Array.from(from.querySelectorAll('link[rel="stylesheet"], style')))
    to.head.appendChild(node.cloneNode(true));
}

/**
 * A pop-out call window (VOX-UI-012) in a Document Picture-in-Picture window: who is in the call, your
 * camera or share, and mute / deafen / disconnect, so the call stays visible above other windows. The
 * returned `portal` is rendered by the caller; closing the window (or leaving the call) tears it down.
 */
export function usePopOut(channelId: string): {
  supported: boolean;
  isOpen: boolean;
  open(): Promise<void>;
  close(): void;
  portal: React.ReactNode;
} {
  const [win, setWin] = useState<Window | null>(null);
  const live = useRef<Window | null>(null);
  const status = useVoiceSession((s) => s.status);

  const close = useCallback(() => {
    const w = live.current;
    live.current = null;
    setWin(null);
    if (w && !w.closed) w.close();
  }, []);

  const open = useCallback(async () => {
    const api = pipApi();
    if (!api || live.current) return;
    const w = await api.requestWindow({ width: 360, height: 420 });
    copyStyles(document, w.document);
    w.addEventListener('pagehide', () => {
      if (live.current === w) {
        live.current = null;
        setWin(null);
      }
    });
    live.current = w;
    setWin(w);
  }, []);

  useEffect(() => {
    if (status === 'idle' || status === 'error') close();
  }, [status, close]);
  useEffect(() => close, [close]);

  return {
    supported: isPopOutSupported(),
    isOpen: !!win,
    open,
    close,
    portal: win
      ? createPortal(<PopOutCall channelId={channelId} onClose={close} />, win.document.body)
      : null,
  };
}

function PopOutCall({ channelId, onClose }: { channelId: string; onClose: () => void }) {
  const t = useT();
  const session = useVoiceSession();
  const members = useVoiceMembers(channelId);
  const users = useUsers();
  const screen = useLocalVideo((s) => s.screen);
  const camera = useLocalVideo((s) => s.camera);
  const controller = voiceController();
  const stream = session.screen ? screen?.stream : session.camera ? camera?.stream : undefined;
  const mutedNow = session.selfMute || session.serverMute;
  const deafNow = session.selfDeaf || session.serverDeaf;
  return (
    <div className="vc-popout" data-testid="voice-popout">
      <h1 className="vc-popout-title">{session.channelName}</h1>
      {stream && <VideoSurface stream={stream} label={t('lounge.popOut.preview')} />}
      <ul className="vc-popout-people" aria-label={t('lounge.popOut.people')}>
        {members.map((m) => {
          const u = users[m.userId];
          const name = u?.displayName ?? t('lounge.voiceChannel.unknown');
          return (
            <li key={m.userId} title={name}>
              <Avatar name={name} src={u?.avatarUrl ?? null} size={40} speaking={m.speaking} />
            </li>
          );
        })}
      </ul>
      <div
        className="vc-popout-controls"
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
          aria-label={deafNow ? t('lounge.voiceChannel.undeafen') : t('lounge.voiceChannel.deafen')}
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
          aria-label={t('lounge.voiceChannel.disconnect')}
          onClick={() => {
            controller.leave();
            onClose();
          }}
        >
          <Icon name="logout" size={24} />
        </button>
      </div>
    </div>
  );
}
