// Things drawn over the live picture: the channel OSD (TV-PLY-002), the now/next info banner with a progress
// bar, the channel list overlay (G) and the tuning static (TV-PLY-003).
import { useEffect, useRef, useState } from 'react';
import { nowAndNext, programProgress, timeLabel } from './model';
import type { TvBlock, TvChannel } from './types';

/** Big channel number, call sign and programme right after tuning; fades after three seconds. */
export function ChannelOsd({
  channel,
  title,
  visible,
}: {
  channel: TvChannel | null;
  title: string | null;
  visible: boolean;
}) {
  if (!channel || !visible) return null;
  return (
    <div className="ltv-osd" role="status" aria-live="polite">
      <span className="ltv-osd-num">{channel.number}</span>
      <span className="ltv-osd-call">{channel.call_sign}</span>
      {title && <span className="ltv-osd-title">{title}</span>}
    </div>
  );
}

/** On-screen programme info: what is on, a progress bar, what is next. */
export function InfoBanner({
  channel,
  now,
  next,
  nowMs,
}: {
  channel: TvChannel | null;
  now: TvBlock | null;
  next: TvBlock | null;
  nowMs: number;
}) {
  const pct = Math.round(programProgress(now, nowMs) * 100);
  return (
    <section className="ltv-banner" aria-label="Programme information">
      <div className="ltv-banner-main">
        <p className="ltv-banner-kicker">
          {channel ? `${channel.number} · ${channel.call_sign}` : 'Live'}
          {now ? ` · ${timeLabel(now.start_ms)} to ${timeLabel(now.end_ms)}` : ''}
        </p>
        <p className="ltv-banner-title">
          <span className="ltv-chip">NOW</span> {now ? now.title : 'Off the air'}
          {now?.episode_label && <span className="ltv-banner-ep"> · {now.episode_label}</span>}
        </p>
        {now && (
          <div
            className="ltv-progress"
            role="progressbar"
            aria-label="Programme progress"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct}
            aria-valuetext={`${pct} percent through ${now.title}`}
          >
            <div style={{ width: `${pct}%` }} />
          </div>
        )}
      </div>
      <p className="ltv-banner-next">
        <span className="ltv-chip ltv-chip-dim">NEXT</span>{' '}
        {next ? `${timeLabel(next.start_ms)}  ${next.title}` : 'Nothing scheduled'}
      </p>
    </section>
  );
}

export interface ChannelListProps {
  channels: TvChannel[];
  blocks: Record<string, TvBlock[]>;
  currentId: string;
  nowMs: number;
  onTune: (channelId: string) => void;
  onClose: () => void;
}

/** The mini guide: every channel with what is on and next. A listbox: arrows move, Enter tunes, Escape closes. */
export function ChannelList({
  channels,
  blocks,
  currentId,
  nowMs,
  onTune,
  onClose,
}: ChannelListProps) {
  const list = [...channels].sort((a, b) => a.number - b.number);
  const [active, setActive] = useState(() =>
    Math.max(
      0,
      list.findIndex((c) => c.id === currentId),
    ),
  );
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.focus();
  }, []);
  useEffect(() => {
    box.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView?.({ block: 'nearest' });
  }, [active]);
  const activeId = list[active] ? `ltv-ch-${list[active].id}` : undefined;
  return (
    <section className="ltv-guide" role="dialog" aria-label="Channel guide">
      <h2 className="ltv-guide-title">Channels</h2>
      <div
        ref={box}
        role="listbox"
        tabIndex={0}
        aria-label="Channels"
        aria-activedescendant={activeId}
        className="ltv-guide-list"
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') {
            e.preventDefault();
            e.stopPropagation();
            setActive((i) => Math.min(list.length - 1, i + 1));
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            e.stopPropagation();
            setActive((i) => Math.max(0, i - 1));
          } else if (e.key === 'Enter') {
            e.preventDefault();
            e.stopPropagation();
            const c = list[active];
            if (c) onTune(c.id);
          } else if (e.key === 'Escape' || e.key === 'g' || e.key === 'G') {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          }
        }}
      >
        {list.map((c, i) => {
          const { now, next } = nowAndNext(blocks[c.id], nowMs);
          return (
            <div
              key={c.id}
              id={`ltv-ch-${c.id}`}
              role="option"
              aria-selected={i === active}
              data-current={c.id === currentId || undefined}
              className="ltv-guide-item"
              onClick={() => onTune(c.id)}
            >
              <span className="ltv-guide-num">{c.number}</span>
              <span className="ltv-guide-call">{c.call_sign}</span>
              <span className="ltv-guide-now">{now ? now.title : 'Off the air'}</span>
              <span className="ltv-guide-next">
                {next ? `Next ${timeLabel(next.start_ms)} ${next.title}` : ''}
              </span>
            </div>
          );
        })}
        {list.length === 0 && <p className="ltv-guide-empty">No channels to show.</p>}
      </div>
      <button type="button" className="cpl-btn ltv-guide-close" onClick={onClose}>
        Close
      </button>
    </section>
  );
}

/** Noise over the picture while changing channel. Not animated under reduced motion. */
export function TuningStatic({ visible }: { visible: boolean }) {
  if (!visible) return null;
  return <div className="ltv-static" data-testid="tuning-static" aria-hidden="true" />;
}
