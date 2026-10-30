// Things drawn over the live picture: the channel OSD (TV-PLY-002), the now/next info banner with a progress
// bar, the channel list overlay (G) and the tuning static (TV-PLY-003).
import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { compareChannels, nowAndNext, programProgress, timeLabel } from './model';
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
  const t = useT();
  const pct = Math.round(programProgress(now, nowMs) * 100);
  return (
    <section className="ltv-banner" aria-label={t('cinema.live.programmeInfo')}>
      <div className="ltv-banner-main">
        <p className="ltv-banner-kicker">
          {channel
            ? t('cinema.live.channelLine', { number: channel.number, callSign: channel.call_sign })
            : t('cinema.live.liveShort')}
          {now
            ? t('cinema.live.timeSuffix', {
                start: timeLabel(now.start_ms),
                end: timeLabel(now.end_ms),
              })
            : ''}
        </p>
        <p className="ltv-banner-title">
          <span className="ltv-chip">{t('cinema.live.nowChip')}</span>{' '}
          {now ? now.title : t('cinema.live.offAir')}
          {now?.episode_label && <span className="ltv-banner-ep"> · {now.episode_label}</span>}
        </p>
        {now && (
          <div
            className="ltv-progress"
            role="progressbar"
            aria-label={t('cinema.live.programProgress')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct}
            aria-valuetext={t('cinema.live.progressText', { pct, title: now.title })}
          >
            <div style={{ width: `${pct}%` }} />
          </div>
        )}
      </div>
      <p className="ltv-banner-next">
        <span className="ltv-chip ltv-chip-dim">{t('cinema.live.nextChip')}</span>{' '}
        {next
          ? t('cinema.live.nextLine', { time: timeLabel(next.start_ms), title: next.title })
          : t('cinema.live.nothingScheduled')}
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
  const t = useT();
  const list = [...channels].sort(compareChannels);
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
    <section className="ltv-guide" role="dialog" aria-label={t('cinema.live.channelGuide')}>
      <h2 className="ltv-guide-title">{t('cinema.live.channels')}</h2>
      <div
        ref={box}
        role="listbox"
        tabIndex={0}
        aria-label={t('cinema.live.channels')}
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
              <span className="ltv-guide-now">{now ? now.title : t('cinema.live.offAir')}</span>
              <span className="ltv-guide-next">
                {next
                  ? t('cinema.live.guideNext', {
                      time: timeLabel(next.start_ms),
                      title: next.title,
                    })
                  : ''}
              </span>
            </div>
          );
        })}
        {list.length === 0 && <p className="ltv-guide-empty">{t('cinema.live.noChannels')}</p>}
      </div>
      <button type="button" className="cpl-btn ltv-guide-close" onClick={onClose}>
        {t('cinema.live.close')}
      </button>
    </section>
  );
}

/** Noise over the picture while changing channel. Not animated under reduced motion. */
export function TuningStatic({ visible }: { visible: boolean }) {
  if (!visible) return null;
  return <div className="ltv-static" data-testid="tuning-static" aria-hidden="true" />;
}

/** The captions over the instance slate loop while a channel is off air (D-266); the picture is the stream. */
export function SlateCaption({ next }: { next: TvBlock | null }) {
  const tr = useT();
  return (
    <div className="ltv-slate-caption" role="status" data-testid="slate-caption">
      <p className="ltv-slate-caption-title">{tr('cinema.nowPanel.offAir')}</p>
      <p className="ltv-slate-caption-body">
        {next
          ? tr('cinema.nowPanel.offAirNext', { title: next.title })
          : tr('cinema.nowPanel.offAirBody')}
      </p>
    </div>
  );
}

/** The client-drawn slate for blocks with no media: colour bars and the block's own title. */
export function Slate({ title }: { title: string }) {
  return (
    <div className="ltv-slate" role="status" data-testid="slate">
      <div className="ltv-bars" aria-hidden="true" />
      <p className="ltv-slate-title">{title}</p>
    </div>
  );
}
