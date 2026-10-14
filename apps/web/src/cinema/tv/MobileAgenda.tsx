// Phone layout of the guide (TV-UI-010): one agenda list per channel, the programme on now first, then what is
// coming. Tapping a programme opens the same detail as the grid.
import { Icon } from '@sideroom/ui';
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
}

const PER_CHANNEL = 6;

export function MobileAgenda(p: MobileAgendaProps) {
  return (
    <ul className="tv-agenda" aria-label="Channels">
      {p.channels.map((ch) => {
        const tz = p.tzLocal ? null : ch.timezone;
        const from = Math.max(p.dayStart, Math.min(p.nowMs, p.dayEnd));
        const list = (p.blocks[ch.id] ?? [])
          .filter((b) => b.end_ms > from && b.start_ms < p.dayEnd)
          .slice(0, PER_CHANNEL);
        const fav = p.favourites.has(ch.id);
        return (
          <li key={ch.id} className="tv-agenda-ch">
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
                aria-label={`${fav ? 'Remove' : 'Add'} ${ch.name} ${fav ? 'from' : 'to'} favorites`}
                onClick={() => p.onToggleFavourite(ch.id)}
              >
                <Icon name="star" size={20} className={fav ? 'tv-fav-on' : undefined} />
              </button>
              <a className="tv-agenda-tune" href={watchChannelHref(ch.id)}>
                Tune in<span className="cin-sr-only"> to channel {ch.number}</span>
              </a>
            </div>
            {list.length === 0 ? (
              <p className="cin-muted tv-agenda-empty">Nothing scheduled in this window.</p>
            ) : (
              <ul className="tv-agenda-list" aria-label={`${ch.name} schedule`}>
                {list.map((b) => (
                  <li key={b.id}>
                    <button
                      type="button"
                      className="tv-agenda-item"
                      data-now={isNow(b, p.nowMs) || undefined}
                      onClick={() => p.onOpen(b, ch)}
                    >
                      <span className="tv-cell-time">
                        {isNow(b, p.nowMs) ? 'NOW' : timeLabel(b.start_ms, tz)}
                      </span>
                      <span className="tv-agenda-title">
                        {b.title}
                        {b.episode_label && <span className="tv-cell-ep"> {b.episode_label}</span>}
                      </span>
                      <span className="tv-cell-meta">
                        {kindYear(b)}
                        {b.content_rating && <span className="tv-rating">{b.content_rating}</span>}
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
  );
}
