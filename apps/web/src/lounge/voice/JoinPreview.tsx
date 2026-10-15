import { useEffect, useRef, useState } from 'react';
import { Button } from '@sideroom/ui';
import { useT } from '../../i18n';
import {
  browserVideoDeps,
  FREE_PROFILES,
  openCamera,
  stopCapture,
  type VideoDeps,
  type VideoProfile,
} from '../../voice/video';
import { VideoSurface } from './VideoSurface';
import './voice-p10.css';

/**
 * The "Join Voice" camera preview (VOX-UI-002): a self-view the viewer can switch on before joining. The
 * stream is released when the preview is closed, the screen unmounts, or the call begins.
 */
export function JoinPreview({
  deps = browserVideoDeps,
  profiles = FREE_PROFILES,
}: {
  deps?: VideoDeps;
  profiles?: readonly VideoProfile[];
}) {
  const t = useT();
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const current = useRef<MediaStream | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      stopCapture(current.current);
      current.current = null;
    };
  }, []);

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const cam = await openCamera(deps, 'default', profiles);
      if (!alive.current) return stopCapture(cam.stream);
      current.current = cam.stream;
      setStream(cam.stream);
    } catch (e) {
      if (alive.current) setError(e instanceof Error ? e.message : t('voice.capture.cameraFailed'));
    } finally {
      if (alive.current) setBusy(false);
    }
  };
  const stop = () => {
    stopCapture(current.current);
    current.current = null;
    setStream(null);
  };

  return (
    <div className="vc-preview" data-testid="join-preview">
      {stream ? (
        <>
          <VideoSurface stream={stream} mirror label={t('lounge.joinPreview.label')} />
          <Button size="sm" variant="secondary" onClick={stop}>
            {t('lounge.joinPreview.off')}
          </Button>
        </>
      ) : (
        <Button size="sm" variant="secondary" loading={busy} onClick={() => void start()}>
          {t('lounge.joinPreview.on')}
        </Button>
      )}
      {error && (
        <p role="alert" className="vc-inline-error">
          {error}
        </p>
      )}
    </div>
  );
}
