// The time grid (TV-UI-005): channel column, half-hour header, duration-sized programme cells, now line. One
// horizontal scroller; the channel column is sticky. Arrow keys move focus between cells (roving tabindex),
// Enter opens the programme, `n` jumps to now.
import { Icon } from '@sideroom/ui';
import { forwardRef, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { t, useT } from '../../i18n';
import { AgeBadge } from '../ui';
import {
  SLOT_MS,
  cellBox,
  isNow,
  kindYear,
  neighbourCell,
  runtimeMinutes,
  formatDuration,
  timeLabel,
  type CellId,
  type NavKey,
} from './model';
import type { TvBlock, TvChannel } from './types';

export type TzMode = 'local' | 'channel';

export interface GuideGridProps {
  channels: TvChannel[];
  blocks: Record<string, TvBlock[]>;
  nowMs: number;
  dayStart: number;
  dayEnd: number;
  slotPx: number;
  tzMode: TzMode;
  favourites: ReadonlySet<string>;
  selectedChannelId: string | null;
  onToggleFavourite: (channelId: string) => void;
  onSelectChannel: (channelId: string) => void;
  onOpen: (block: TvBlock, el: HTMLElement) => void;
  /** `n`: jump to now (the page scrolls the grid). */
  onJumpToNow: () => void;
}

const NAV_KEYS: NavKey[] = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'];

export function cellLabel(b: TvBlock, ch: TvChannel, tz: string | null, nowMs: number) {
  const when = isNow(b, nowMs)
    ? t('cinema.guide.onNow')
    : b.start_ms < nowMs
      ? t('cinema.guide.aired')
      : t('cinema.guide.upcoming');
  return [
    `${b.title}`,
    b.episode_label,
    t('cinema.guide.timeRange', {
      start: timeLabel(b.start_ms, tz),
      end: timeLabel(b.end_ms, tz),
    }),
    b.content_rating ? t('cinema.guide.rated', { rating: b.content_rating }) : null,
    b.age_restricted ? t('cinema.guide.ageRestricted') : null,
    t('cinema.guide.channelLabel', { number: ch.number, callSign: ch.call_sign }),
    when,
  ]
    .filter(Boolean)
    .join(', ');
}

export const GuideGrid = forwardRef<HTMLDivElement, GuideGridProps>(function GuideGrid(
  {
    channels,
    blocks,
    nowMs,
    dayStart,
    dayEnd,
    slotPx,
    tzMode,
    favourites,
    selectedChannelId,
    onToggleFavourite,
    onSelectChannel,
    onOpen,
    onJumpToNow,
  },
  scrollerRef,
) {
  const tr = useT();
  const rows = useMemo(
    () =>
      channels.map((c) =>
        (blocks[c.id] ?? []).filter((b) => b.end_ms > dayStart && b.start_ms < dayEnd),
      ),
    [channels, blocks, dayStart, dayEnd],
  );
  const slots = Math.max(1, Math.ceil((dayEnd - dayStart) / SLOT_MS));
  const trackW = slots * slotPx;
  const nowLeft = ((nowMs - dayStart) / SLOT_MS) * slotPx;
  const nowVisible = nowMs >= dayStart && nowMs < dayEnd;

  // Roving focus: the cell that airs now in the first row that has one, else the first cell.
  const defaultCell = useMemo<CellId | null>(() => {
    for (let r = 0; r < rows.length; r++) {
      const hit = rows[r]!.find((b) => isNow(b, nowMs));
      if (hit) return { row: r, id: hit.id };
    }
    for (let r = 0; r < rows.length; r++) {
      const first = rows[r]![0];
      if (first) return { row: r, id: first.id };
    }
    return null;
  }, [rows, nowMs]);
  const [focus, setFocus] = useState<CellId | null>(null);
  const active = useMemo(() => {
    if (focus && rows[focus.row]?.some((b) => b.id === focus.id)) return focus;
    return defaultCell;
  }, [focus, rows, defaultCell]);
  const pending = useRef<string | null>(null);
  const root = useRef<HTMLDivElement | null>(null);

  // Move DOM focus after the roving index changed through the keyboard.
  useEffect(() => {
    if (!pending.current) return;
    const el = root.current?.querySelector<HTMLElement>(`[data-block="${pending.current}"]`);
    pending.current = null;
    el?.focus();
    el?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [focus]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'n' || e.key === 'N') {
      e.preventDefault();
      onJumpToNow();
      return;
    }
    if (!NAV_KEYS.includes(e.key as NavKey) || !active) return;
    const from = (e.target as HTMLElement).closest<HTMLElement>('[data-block]');
    if (!from) return;
    const id = from.dataset.block!;
    const row = Number(from.dataset.row);
    const next = neighbourCell(rows, { row, id }, e.key as NavKey);
    e.preventDefault();
    if (next) {
      pending.current = next.id;
      setFocus(next);
    }
  };

  const headerSlots = Array.from({ length: slots }, (_, i) => dayStart + i * SLOT_MS);

  return (
    <div className="tv-scroller" ref={scrollerRef} data-testid="tv-scroller">
      <div
        ref={root}
        role="grid"
        aria-label={tr('cinema.guide.programmeGuide')}
        aria-rowcount={channels.length + 1}
        className="tv-grid"
        style={{
          ['--tv-slot' as string]: `${slotPx}px`,
          width: `calc(var(--tv-chan) + ${trackW}px)`,
        }}
        onKeyDown={onKeyDown}
      >
        <div role="row" className="tv-row tv-head">
          <div role="columnheader" className="tv-chan tv-chan-head">
            <span className="tv-chan-head-title">{tr('cinema.guide.yourChannels')}</span>
            <span className="tv-chan-head-now">
              {tr('cinema.guide.nowAt', { time: timeLabel(nowMs) })}
            </span>
          </div>
          <div role="presentation" className="tv-track" style={{ width: trackW }}>
            {headerSlots.map((slot, i) => (
              <div
                key={slot}
                role="columnheader"
                className="tv-slot-head"
                style={{ left: i * slotPx, width: slotPx }}
              >
                {timeLabel(slot)}
              </div>
            ))}
          </div>
        </div>

        {rows.map((list, r) => {
          const ch = channels[r]!;
          const tz = tzMode === 'channel' ? ch.timezone : null;
          const fav = favourites.has(ch.id);
          return (
            <div
              role="row"
              aria-rowindex={r + 2}
              className="tv-row"
              key={ch.id}
              data-selected={selectedChannelId === ch.id || undefined}
            >
              <div role="rowheader" className="tv-chan">
                <button
                  type="button"
                  className="tv-fav"
                  aria-pressed={fav}
                  aria-label={
                    fav
                      ? tr('cinema.guide.removeFavorite', { name: ch.name })
                      : tr('cinema.guide.addFavorite', { name: ch.name })
                  }
                  onClick={() => onToggleFavourite(ch.id)}
                >
                  <Icon name="star" size={16} className={fav ? 'tv-fav-on' : undefined} />
                </button>
                <span className="tv-chan-num">{ch.number}</span>
                <button
                  type="button"
                  className="tv-chan-name"
                  aria-label={tr('cinema.guide.showInPreview', {
                    number: ch.number,
                    callSign: ch.call_sign,
                  })}
                  onClick={() => onSelectChannel(ch.id)}
                >
                  {ch.logo_url ? (
                    <img src={ch.logo_url} alt="" className="tv-chan-logo" />
                  ) : (
                    <span className="tv-call">{ch.call_sign}</span>
                  )}
                  <span className="tv-chan-sub">{ch.network ?? ch.name}</span>
                </button>
              </div>
              <div role="presentation" className="tv-track" style={{ width: trackW }}>
                {list.map((b) => {
                  const { left, width } = cellBox(b, dayStart, slotPx);
                  const live = isNow(b, nowMs);
                  const roving = active?.id === b.id;
                  const min = b.runtime_min ?? runtimeMinutes(b);
                  return (
                    <div
                      role="gridcell"
                      key={b.id}
                      className="tv-cell-wrap"
                      style={{ left, width }}
                    >
                      <button
                        type="button"
                        className="tv-cell"
                        data-block={b.id}
                        data-row={r}
                        data-now={live || undefined}
                        data-past={b.end_ms <= nowMs || undefined}
                        data-filler={b.kind === 'filler' || undefined}
                        tabIndex={roving ? 0 : -1}
                        aria-label={cellLabel(b, ch, tz, nowMs)}
                        aria-haspopup="dialog"
                        onFocus={() => setFocus({ row: r, id: b.id })}
                        onClick={(e) => onOpen(b, e.currentTarget)}
                      >
                        <span className="tv-cell-time" aria-hidden="true">
                          {live
                            ? tr('cinema.guide.nowTime', { time: timeLabel(b.start_ms, tz) })
                            : timeLabel(b.start_ms, tz)}
                        </span>
                        <span className="tv-cell-title" aria-hidden="true">
                          {b.title}
                        </span>
                        {b.episode_label && (
                          <span className="tv-cell-ep" aria-hidden="true">
                            {b.episode_label}
                          </span>
                        )}
                        <span className="tv-cell-meta" aria-hidden="true">
                          {kindYear(b)} · {formatDuration(min)}
                          {b.content_rating && (
                            <span className="tv-rating">{b.content_rating}</span>
                          )}
                          {b.age_restricted && <AgeBadge className="tv-age" />}
                        </span>
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
        {nowVisible && (
          <div
            className="tv-nowline"
            aria-hidden="true"
            data-testid="tv-nowline"
            style={{ left: `calc(var(--tv-chan) + ${nowLeft}px)` }}
          />
        )}
      </div>
    </div>
  );
});
