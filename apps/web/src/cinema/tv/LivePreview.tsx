// Muted live preview inside the CRT frame (TV-UI-002). Lazily imported, so the guide page does not ship the HLS
// engine until a viewer actually hovers or focuses the preview. Failures leave the still picture in place.
import { useEffect, useRef } from 'react';
import { createEngine, type PlaybackEngine } from '../player/engine';
import { tuneChannel } from './api';

export function LivePreview({
  channelId,
  onState,
}: {
  channelId: string;
  /** `live` once frames play; `error` when the channel could not be tuned. */
  onState?: (s: 'live' | 'error') => void;
}) {
  const ref = useRef<HTMLVideoElement>(null);
  const cb = useRef(onState);
  cb.current = onState;

  useEffect(() => {
    const video = ref.current;
    if (!video) return;
    let dead = false;
    let engine: PlaybackEngine | null = null;
    void (async () => {
      try {
        const grant = await tuneChannel(channelId);
        if (dead) return;
        engine = await createEngine(video);
        if (dead) {
          engine.destroy();
          return;
        }
        engine.load(grant.master_url);
        video.muted = true;
        await video.play().catch(() => undefined);
        if (!dead) cb.current?.('live');
      } catch {
        if (!dead) cb.current?.('error');
      }
    })();
    return () => {
      dead = true;
      engine?.destroy();
      video.removeAttribute('src');
    };
  }, [channelId]);

  return <video ref={ref} className="tv-crt-video" muted playsInline aria-hidden="true" />;
}
