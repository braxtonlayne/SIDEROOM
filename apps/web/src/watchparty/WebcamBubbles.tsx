// Webcam bubbles over the watch party stage (WP-FE-012). When the viewer is also in a voice call, the cameras
// of the call (their own and everyone who has video on) float over the picture as small round bubbles. It is
// shown by default (D-265); "Hide cameras" in the corner turns it off and the choice is an account preference
// (`watchPartyCameras`). Outside a call it shows nothing at all. Bubbles are thumbnail-sized and make no extra
// subscription (VideoSurface reports no viewport).
import { Button, Icon } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { VideoSurface } from '../lounge/voice/VideoSurface';
import { usePrefsStore } from '../settings/user/prefs';
import { useStore } from '../store/store';
import { useVoiceSession } from '../voice/session';
import { useLocalVideo, useRemoteVideo } from '../voice/video';

export function WebcamBubbles() {
  const t = useT();
  const inCall = useVoiceSession((s) => s.status === 'connected');
  const localCamera = useLocalVideo((s) => s.camera);
  const cameraOn = useVoiceSession((s) => s.camera);
  const remote = useRemoteVideo((s) => s.camera);
  const videoOff = useRemoteVideo((s) => s.videoOff);
  const users = useStore((s) => s.users);
  const prefs = usePrefsStore((s) => s.prefs);
  const save = usePrefsStore((s) => s.save);
  const on = prefs.watchPartyCameras;
  // Pressing "Show cameras" with nobody on camera is acknowledged with the status line, for this view only.
  const [asked, setAsked] = useState(false);
  if (!inCall) return null;

  const nameOf = (id: string) => {
    const u = users[id];
    return u?.global_name || u?.username || t('watchparty.bubbles.someone');
  };
  const remoteIds = Object.keys(remote).filter((id) => !videoOff[id]);
  const self = cameraOn && localCamera ? localCamera.stream : null;
  const count = remoteIds.length + (self ? 1 : 0);

  return (
    <div className="wp-bubbles" data-testid="wp-bubbles">
      <Button
        size="sm"
        variant="secondary"
        className="wp-bubbles-toggle"
        aria-pressed={on}
        onClick={() => {
          setAsked(!on);
          save({ ...prefs, watchPartyCameras: !on });
        }}
      >
        <Icon name={on ? 'video' : 'video-off'} size={16} />
        {on ? t('watchparty.bubbles.hide') : t('watchparty.bubbles.show')}
      </Button>
      {on && count === 0 && asked && (
        <p className="wp-bubble-empty" role="status">
          {t('watchparty.bubbles.none')}
        </p>
      )}
      {on && count > 0 && (
        <ul className="wp-bubble-list" aria-label={t('watchparty.bubbles.label')}>
          {self && (
            <li className="wp-bubble" data-testid="wp-bubble-self">
              <VideoSurface
                stream={self}
                mirror
                muted
                className="wp-bubble-video"
                label={t('watchparty.bubbles.yours')}
              />
            </li>
          )}
          {remoteIds.map((id) => (
            <li key={id} className="wp-bubble" data-testid={`wp-bubble-${id}`}>
              {/* Their voice already plays through the call; the bubble is picture only. */}
              <VideoSurface
                stream={remote[id]!}
                muted
                className="wp-bubble-video"
                label={t('watchparty.bubbles.of', { name: nameOf(id) })}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
