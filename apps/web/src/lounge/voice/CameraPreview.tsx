import { Button, Modal, Notice } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import {
  browserVideoDeps,
  cameraConstraints,
  stopCapture,
  toCaptureError,
  type VideoDeps,
} from '../../voice/video';
import './voice-p10.css';

const PREVIEW_PROFILE = { height: 360, fps: 15 } as const;

/**
 * A live look at a camera that is not sent to anyone (LNG-USET-047). Opens the camera while mounted and stops
 * every track when it goes away, so the camera light is off as soon as the preview is.
 */
export function CameraPreview({
  deviceId,
  deps = browserVideoDeps,
}: {
  deviceId: string;
  deps?: Pick<VideoDeps, 'getUserMedia'>;
}) {
  const t = useT();
  const video = useRef<HTMLVideoElement | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const el = video.current;
    let cancelled = false;
    let stream: MediaStream | null = null;
    setError(null);
    deps
      .getUserMedia(cameraConstraints(deviceId, PREVIEW_PROFILE))
      .then((s) => {
        if (cancelled) return stopCapture(s);
        stream = s;
        if (video.current) {
          video.current.srcObject = s;
          void video.current.play?.()?.catch?.(() => undefined);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(toCaptureError(e, 'camera').message);
      });
    return () => {
      cancelled = true;
      if (stream) stopCapture(stream);
      if (el) el.srcObject = null;
    };
  }, [deviceId, deps]);
  if (error)
    return (
      <Notice variant="warning" role="alert">
        {error}
      </Notice>
    );
  return (
    <video
      ref={video}
      className="vc-camera-preview"
      muted
      playsInline
      aria-label={t('lounge.cameraPreview.label')}
    />
  );
}

/** "Always preview video": look first, then go live (Settings > Voice & Video). */
export function CameraPreviewDialog({
  open,
  deviceId,
  onClose,
  onStart,
}: {
  open: boolean;
  deviceId: string;
  onClose: () => void;
  onStart: () => void;
}) {
  const t = useT();
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('lounge.cameraPreview.title')}
      description={t('lounge.cameraPreview.description')}
      size="sm"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.cameraPreview.cancel')}
          </Button>
          <Button
            onClick={() => {
              onStart();
              onClose();
            }}
          >
            {t('lounge.cameraPreview.start')}
          </Button>
        </>
      }
    >
      {open && <CameraPreview deviceId={deviceId} />}
    </Modal>
  );
}
