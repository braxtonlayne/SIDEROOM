import { useEffect, useRef } from 'react';

/** A `<video>` showing a MediaStream (a camera, a screen share, or someone else's video). */
export function VideoSurface({
  stream,
  muted = true,
  mirror = false,
  label,
  className,
  volume,
}: {
  stream: MediaStream;
  /** Local previews are muted; a watched remote stream is not. */
  muted?: boolean;
  /** Mirror a self-view camera, like Discord. */
  mirror?: boolean;
  label: string;
  className?: string;
  /** 0 to 1; a watched stream follows the viewer's volume for that person. */
  volume?: number | undefined;
}) {
  const ref = useRef<HTMLVideoElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (el && el.srcObject !== stream) el.srcObject = stream;
    return () => {
      if (el) el.srcObject = null;
    };
  }, [stream]);
  useEffect(() => {
    if (ref.current && volume !== undefined) ref.current.volume = Math.min(1, Math.max(0, volume));
  }, [volume]);
  return (
    <video
      ref={ref}
      className={className ?? 'vc-video'}
      data-mirror={mirror || undefined}
      aria-label={label}
      autoPlay
      playsInline
      muted={muted}
    />
  );
}
