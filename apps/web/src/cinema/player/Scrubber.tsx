// The seek bar (CIN-PLY-002): a native range input (keyboard and screen readers for free) over a buffered/played
// track, marker ticks for intro and credits, and a hover/focus preview with the sprite thumbnail.
import { useQuery } from '@tanstack/react-query';
import { useRef, useState, type PointerEvent } from 'react';
import { useT } from '../../i18n';
import { getSprites } from './api';
import { clamp, formatClock, spokenClock, spriteAt } from './model';
import type { Marker } from './types';

export interface ScrubberProps {
  currentTime: number;
  duration: number;
  bufferedEnd: number;
  markers: Marker[];
  spritesUrl: string | null;
  onSeek: (t: number) => void;
}

export function Scrubber({
  currentTime,
  duration,
  bufferedEnd,
  markers,
  spritesUrl,
  onSeek,
}: ScrubberProps) {
  const tr = useT();
  const wrap = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ t: number; x: number } | null>(null);
  const sprites = useQuery({
    queryKey: ['cinema', 'sprites', spritesUrl],
    queryFn: ({ signal }) => getSprites(spritesUrl!, signal),
    enabled: !!spritesUrl,
    staleTime: 30 * 60_000,
  });
  const dur = duration > 0 ? duration : 0;
  const pct = (t: number) => (dur > 0 ? clamp((t / dur) * 100, 0, 100) : 0);

  const onMove = (e: PointerEvent) => {
    const r = wrap.current?.getBoundingClientRect();
    if (!r || r.width === 0 || dur === 0) return;
    const f = clamp((e.clientX - r.left) / r.width, 0, 1);
    setHover({ t: f * dur, x: f * r.width });
  };
  const cue = hover && sprites.data ? spriteAt(sprites.data, hover.t) : null;

  return (
    <div
      ref={wrap}
      className="cpl-scrub"
      onPointerMove={onMove}
      onPointerLeave={() => setHover(null)}
    >
      <div className="cpl-scrub-track" aria-hidden="true">
        <div className="cpl-scrub-buffer" style={{ width: `${pct(bufferedEnd)}%` }} />
        <div className="cpl-scrub-played" style={{ width: `${pct(currentTime)}%` }} />
        {markers
          .filter((m) => m.kind === 'intro' || m.kind === 'credits')
          .map((m) => (
            <span
              key={`${m.kind}-${m.start_s}`}
              className="cpl-scrub-marker"
              data-kind={m.kind}
              style={{ left: `${pct(m.start_s)}%`, width: `${pct(m.end_s) - pct(m.start_s)}%` }}
            />
          ))}
      </div>
      <input
        type="range"
        className="cpl-scrub-input"
        aria-label={tr('cinema.scrubber.seek')}
        min={0}
        max={dur || 1}
        step={1}
        value={clamp(currentTime, 0, dur || 1)}
        aria-valuetext={tr('cinema.scrubber.valueText', {
          current: spokenClock(currentTime),
          total: spokenClock(dur),
        })}
        disabled={dur === 0}
        onChange={(e) => onSeek(Number(e.target.value))}
      />
      {hover && (
        <div
          className="cpl-scrub-preview"
          aria-hidden="true"
          style={{
            left: `clamp(${(cue?.w ?? 80) / 2}px, ${hover.x}px, calc(100% - ${(cue?.w ?? 80) / 2}px))`,
          }}
        >
          {cue && (
            <div
              className="cpl-scrub-thumb"
              data-testid="scrub-thumb"
              style={{
                width: cue.w,
                height: cue.h,
                backgroundImage: `url(${JSON.stringify(cue.url)})`,
                backgroundPosition: `${-cue.x}px ${-cue.y}px`,
              }}
            />
          )}
          <span className="cpl-scrub-time">{formatClock(hover.t)}</span>
        </div>
      )}
    </div>
  );
}
