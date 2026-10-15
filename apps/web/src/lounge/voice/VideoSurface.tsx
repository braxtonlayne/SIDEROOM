import { useEffect, useRef } from 'react';

/** A `<video>` showing a MediaStream (a camera, a screen share, or someone else's video). */
export function VideoSurface({
  stream,
  muted = true,
  mirror = false,
  label,
  className,
}: {
  stream: MediaStream;
  /** Local previews are muted; a watched remote stream is not. */
  muted?: boolean;
  /** Mirror a self-view camera, like Discord. */
  mirror?: boolean;
  label: string;
  className?: string;
}) {
  const ref = useRef<HTMLVideoElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (el && el.srcObject !== stream) el.srcObject = stream;
    return () => {
      if (el) el.srcObject = null;
    };
  }, [stream]);
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
