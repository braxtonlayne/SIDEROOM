// SEC-AUD-226 (D-709, D-710, D-719): the title editor says where the subtitle languages come from, the two examples
// in the subtitle panel are a catalog key, and a language the server stores in another form is shown as saved.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const CURATOR = [
  'STAFF_BASIC',
  'REVIEW_MEDIA',
  'PUBLISH_MEDIA',
  'MANAGE_COLLECTIONS',
  'MANAGE_TV',
] as const;

const title = (over: Record<string, unknown> = {}) => ({
  id: '10',
  kind: 'movie',
  title: 'Akira',
  original_title: null,
  alt_titles: [],
  year: 1988,
  runtime_min: 124,
  content_rating: 'R',
  synopsis: 'Neo-Tokyo.',
  tagline: null,
  genres: ['Anime'],
  tags: [],
  languages: ['ja'],
  subtitle_languages: ['en'],
  cast: [],
  directors: [],
  writers: [],
  studios: [],
  crew: [],
  age_restricted: false,
  poster_url: '/cdn/cinema/10/poster.webp?ex=1&sig=a',
  backdrop_url: null,
  logo_url: null,
  external_ids: {},
  season_count: 0,
  episode_count: 0,
  has_trailer: false,
  playable: true,
  published: false,
  published_at_ms: null,
  asset_id: null,
  created_at_ms: 1_700_000_000_000,
  updated_at_ms: 1_700_000_000_000,
  ...over,
});
const detailOf = (t = title()) => ({
  title: t,
  seasons: [],
  episodes: [],
  variants: [],
  trailers: [],
  markers: [],
});
const page = (items: unknown[]) => ({ items, total: items.length, next_cursor: null });

describe('the title editor and the subtitle languages (D-709, D-710, D-719)', () => {
  it('says beside the field that visible tracks add their language, and ties the line to the field', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      http.get('*/api/v1/admin/cinema/titles/10', () => HttpResponse.json(detailOf())),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    const field = await screen.findByRole('textbox', { name: /^Subtitle languages/ });
    const said = field.getAttribute('aria-describedby');
    expect(said).toBeTruthy();
    const line = document.getElementById(said!.split(' ')[0]!)!;
    // The three things: added automatically (also for cuts and uploads), never removed by a track, hide the track.
    expect(line).toHaveTextContent(/added here automatically/);
    expect(line).toHaveTextContent(/also for cuts and for files uploaded for this title/);
    expect(line).toHaveTextContent(/never removes one/);
    expect(line).toHaveTextContent(/hide its track/);
    // The other list fields keep their short hint.
    const genres = screen.getByRole('textbox', { name: /^Genres/ });
    expect(document.getElementById(genres.getAttribute('aria-describedby')!)).toHaveTextContent(
      'Separate with commas',
    );
  });

  it('shows the list the server saved: typed eng, fin comes back en, fi', async () => {
    asStaff({ perms: [...CURATOR] });
    let body: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      // The reload answers the same row version: the form must still show what the save answered.
      http.get('*/api/v1/admin/cinema/titles/10', () => HttpResponse.json(detailOf())),
      http.put('*/api/v1/admin/cinema/titles/10', async ({ request }) => {
        body = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(title({ subtitle_languages: ['en', 'fi'] }));
      }),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    const field = await screen.findByRole('textbox', { name: /^Subtitle languages/ });
    await user.clear(field);
    await user.type(field, 'eng, fin');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(body.subtitle_languages).toEqual(['eng', 'fin']));
    await waitFor(() =>
      expect(screen.getByRole('textbox', { name: /^Subtitle languages/ })).toHaveValue('en, fi'),
    );
  });

  it('shows the language a track was saved with, and the example comes from the catalog', async () => {
    asStaff({ perms: [...CURATOR] });
    const track = {
      id: '1',
      label: 'English',
      language: 'en',
      forced: false,
      hearing_impaired: false,
      is_default: true,
      hidden: false,
      source: 'embedded',
      styled: false,
    };
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () =>
        HttpResponse.json(page([title({ asset_id: '777' })])),
      ),
      http.get('*/api/v1/admin/cinema/titles/10', () =>
        HttpResponse.json(detailOf(title({ asset_id: '777' }))),
      ),
      http.get('*/api/v1/admin/cinema/assets/777/subtitles', () =>
        HttpResponse.json({ tracks: [track] }),
      ),
      http.patch('*/api/v1/admin/cinema/assets/777/subtitles/1', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ ...track, language: 'pt-BR' });
      }),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Video and transcoding' }));
    await user.click(await screen.findByRole('button', { name: 'Show the subtitles of Akira' }));
    const block = await screen.findByRole('region', { name: 'Subtitles' });
    const row = await within(block).findByLabelText('Language of track 1');
    expect(row).toHaveAttribute('placeholder', 'en, pt-BR');
    expect(within(block).getByLabelText('Language of the new track')).toHaveAttribute(
      'placeholder',
      'en, pt-BR',
    );
    await user.clear(row);
    await user.type(row, 'POR-br{Enter}');
    await waitFor(() => expect(sent).toEqual({ language: 'POR-br' }));
    await waitFor(() => expect(row).toHaveValue('pt-BR'));
  });
});
