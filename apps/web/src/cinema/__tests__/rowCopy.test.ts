// SEC-AUD-040: rows the server builds itself show translated copy; anything else is shown as sent.
import { describe, expect, it } from 'vitest';
import { rowTitle } from '../rowCopy';

const t = (key: string, v?: Record<string, string>) =>
  `[${key}${v ? ` ${JSON.stringify(v)}` : ''}]`;

describe('rowTitle', () => {
  it('translates the default copy of a generated or provided row', () => {
    expect(rowTitle({ key: 'anime-room', source: 'generated', title: 'The anime room' }, t)).toBe(
      '[cinema.rows.animeRoom]',
    );
    expect(rowTitle({ key: 'trending', source: 'provided', title: 'Trending' }, t)).toBe(
      '[cinema.rows.trending]',
    );
    expect(
      rowTitle(
        { key: 'because-you-watched', source: 'provided', title: 'Because you watched Akira' },
        t,
      ),
    ).toBe('[cinema.rows.becauseYouWatched {"title":"Akira"}]');
  });

  it('keeps curated rows, overrides and genre names as the server sent them', () => {
    expect(rowTitle({ key: 'anime-room', source: 'curated', title: 'The anime room' }, t)).toBe(
      'The anime room',
    );
    expect(rowTitle({ key: 'anime-room', source: 'generated', title: 'Anime night' }, t)).toBe(
      'Anime night',
    );
    expect(rowTitle({ key: 'genre:Drama', source: 'generated', title: 'Drama' }, t)).toBe('Drama');
  });
});
