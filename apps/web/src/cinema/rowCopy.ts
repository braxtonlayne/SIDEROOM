/**
 * SEC-AUD-040 (audit theme 11): the Cinema rows the server builds itself (generated and provided rows) carry
 * English default copy (`crates/sr-cinema/src/rows.rs`, `sr-cinema-social/src/hooks.rs`). The client knows those
 * rows by key, so it shows the translated copy instead; any other text (a curated row, a provider or staff
 * override, a genre name) is shown as the server sent it.
 */
import type { Row } from './types';

type T = (key: string, values?: Record<string, string>) => string;

/** Row key -> [the server's English default title, the message key that translates it]. */
const TITLES: Record<string, [string, string]> = {
  'continue-watching': ['Continue watching', 'cinema.rows.continueWatching'],
  'recently-explored': ['Recently explored', 'cinema.rows.recentlyExplored'],
  trending: ['Trending', 'cinema.rows.trending'],
  'friends-watching': ['Friends are watching', 'cinema.rows.friendsWatching'],
  'requests-fulfilled': ['Requests fulfilled', 'cinema.rows.requestsFulfilled'],
  'new-this-week': ['New this week', 'cinema.rows.newThisWeek'],
  'recently-added': ['Recently added', 'cinema.rows.recentlyAdded'],
  'anime-room': ['The anime room', 'cinema.rows.animeRoom'],
  'one-more-episode': ['One more episode', 'cinema.rows.oneMoreEpisode'],
  films: ['For the love of cinema', 'cinema.rows.films'],
};

const BECAUSE = 'Because you watched ';

/** The heading of a row, translated when it is the server's own default copy. */
export function rowTitle(row: Pick<Row, 'key' | 'source' | 'title'>, t: T): string {
  if (row.source === 'curated') return row.title;
  if (row.key === 'because-you-watched') {
    if (row.title.startsWith(BECAUSE))
      return t('cinema.rows.becauseYouWatched', { title: row.title.slice(BECAUSE.length) });
    if (row.title === BECAUSE.trimEnd()) return t('cinema.rows.becauseYouWatchedPlain');
    return row.title;
  }
  const known = TITLES[row.key];
  return known && known[0] === row.title ? t(known[1]) : row.title;
}
