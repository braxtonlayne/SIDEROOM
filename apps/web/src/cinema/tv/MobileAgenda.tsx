// Phone layout of the guide (TV-UI-010): one agenda list per channel, the programme on now first, then what is
// coming. Tapping a programme opens the same detail as the grid. A horizontal swipe on the agenda moves the
// selected channel (the one the now panel shows) to the previous or next one; the controls of each channel (tune in,
// favourite, the programmes) stay as they were and are the keyboard path.
import { Icon, useMediaQuery } from '@sideroom/ui';
import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from 'react';
import { useT } from '../../i18n';
import { AgeBadge } from '../ui';
import { isNow, kindYear, timeLabel, watchChannelHref } from './model';
import type { TvBlock, TvChannel } from './types';

export interface MobileAgendaProps {
  channels: TvChannel[];
  blocks: Record<string, TvBlock[]>;
  nowMs: number;
  dayStart: number;
  dayEnd: number;
  tzLocal: boolean;
  favourites: ReadonlySet<string>;
  onToggleFavourite: (channelId: string) => void;
  onOpen: (block: TvBlock, channel: TvChannel) => void;
  /** The channel the now panel shows (a swipe moves it). */
  selectedChannelId?: string | null;
  onSelectChannel?: (channelId: string) => void;
}

const PER_CHANNEL = 6;
/** A swipe is at least this many pixels sideways and goes further sideways than up or down. */
export const SWIPE_MIN_PX = 48;

/**
 * Pointer events for a horizontal swipe. Vertical scrolling is never blocked: nothing is prevented here and the
 * list says `touch-action: pan-y`, so the browser keeps panning up and down by itself.
 */
function useSwipe(onSwipe: (direction: -1 | 1) => void) {
  const start = useRef<{ id: number; x: number; y: number } | null>(null);
  return {
    onPointerDown: (e: ReactPointerEvent) => {
      // A mouse drag selects text; the gesture is for touch and pen.
      start.current =
        e.pointerType === 'mouse' ? null : { id: e.pointerId, x: e.clientX, y: e.clientY };
    },
    onPointerUp: (e: ReactPointerEvent) => {
      const s = start.current;
      start.current = null;
      if (!s || s.id !== e.pointerId) return;
      const dx = e.clientX - s.x;
      const dy = e.clientY - s.y;
      if (Math.abs(dx) >= SWIPE_MIN_PX && Math.abs(dx) > Math.abs(dy)) onSwipe(dx < 0 ? 1 : -1);
    },
    onPointerCancel: () => {
      start.current = null;
    },
  };
}

export function MobileAgenda(p: MobileAgendaProps) {
  const t = useT();
  const reduced = useMediaQuery('(prefers-reduced-motion: reduce)');
  const swiped = useRef(false);
  const items = useRef(new Map<string, HTMLLIElement>());
  const selectedIndex = p.channels.findIndex((c) => c.id === p.selectedChannelId);
  const swipe = useSwipe((direction) => {
    if (!p.onSelectChannel || p.channels.length === 0) return;
    const from = selectedIndex < 0 ? 0 : selectedIndex;
    const to = Math.min(p.channels.length - 1, Math.max(0, from + direction));
    if (to === from && selectedIndex >= 0) return;
    swiped.current = true;
    p.onSelectChannel(p.channels[to]!.id);
  });
  // After a swipe the new channel comes into view; no animation when the viewer asked for less motion.
  useEffect(() => {
    if (!swiped.current || !p.selectedChannelId) return;
    swiped.current = false;
    items.current
      .get(p.selectedChannelId)
      ?.scrollIntoView?.({ block: 'start', behavior: reduced ? 'auto' : 'smooth' });
  }, [p.selectedChannelId, reduced]);
  const selected = selectedIndex >= 0 ? p.channels[selectedIndex] : null;
  return (
    <>
      <ul className="tv-agenda" aria-label={t('cinema.agenda.channels')} {...swipe}>
        {p.channels.map((ch) => {
          const tz = p.tzLocal ? null : ch.timezone;
          const from = Math.max(p.dayStart, Math.min(p.nowMs, p.dayEnd));
          const list = (p.blocks[ch.id] ?? [])
            .filter((b) => b.end_ms > from && b.start_ms < p.dayEnd)
            .slice(0, PER_CHANNEL);
          const fav = p.favourites.has(ch.id);
          return (
            <li
              key={ch.id}
              className="tv-agenda-ch"
              data-selected={ch.id === p.selectedChannelId || undefined}
              ref={(el) => {
                if (el) items.current.set(ch.id, el);
                else items.current.delete(ch.id);
              }}
            >
              <div className="tv-agenda-head">
                <span className="tv-chan-num">{ch.number}</span>
                <h2 className="tv-agenda-name">
                  {ch.call_sign}
                  <span className="tv-chan-sub"> {ch.network ?? ch.name}</span>
                </h2>
                <button
                  type="button"
                  className="tv-fav"
                  aria-pressed={fav}
                  aria-label={
                    fav
                      ? t('cinema.guide.removeFavorite', { name: ch.name })
                      : t('cinema.guide.addFavorite', { name: ch.name })
                  }
                  onClick={() => p.onToggleFavourite(ch.id)}
                >
                  <Icon name="star" size={20} className={fav ? 'tv-fav-on' : undefined} />
                </button>
                <a className="tv-agenda-tune" href={watchChannelHref(ch.id)}>
                  {t('cinema.agenda.tuneIn')}
                  <span className="cin-sr-only">
                    {t('cinema.agenda.toChannel', { n: ch.number })}
                  </span>
                </a>
              </div>
              {list.length === 0 ? (
                <p className="cin-muted tv-agenda-empty">{t('cinema.agenda.empty')}</p>
              ) : (
                <ul
                  className="tv-agenda-list"
                  aria-label={t('cinema.agenda.schedule', { name: ch.name })}
                >
                  {list.map((b) => (
                    <li key={b.id}>
                      <button
                        type="button"
                        className="tv-agenda-item"
                        data-now={isNow(b, p.nowMs) || undefined}
                        onClick={() => p.onOpen(b, ch)}
                      >
                        <span className="tv-cell-time">
                          {isNow(b, p.nowMs) ? t('cinema.agenda.now') : timeLabel(b.start_ms, tz)}
                        </span>
                        <span className="tv-agenda-title">
                          {b.title}
                          {b.episode_label && (
                            <span className="tv-cell-ep"> {b.episode_label}</span>
                          )}
                        </span>
                        <span className="tv-cell-meta">
                          {kindYear(b)}
                          {b.content_rating && (
                            <span className="tv-rating">{b.content_rating}</span>
                          )}
                          {b.age_restricted && <AgeBadge className="tv-age" />}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      <p className="cin-sr-only" role="status">
        {selected ? t('cinema.agenda.selected', { n: selected.number, name: selected.name }) : ''}
      </p>
    </>
  );
}
