import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { keyFromUrl } from '../cinema/api';
import { move } from '../cinema/CollectionsSection';
import { rowsToMarkers, validateRows } from '../cinema/MarkersPanel';
import { vi } from 'vitest';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const putFile = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>(async () => undefined));
vi.mock('../../lounge/msg/upload', () => ({ putFile }));

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
const detailOf = (t = title(), over: Record<string, unknown> = {}) => ({
  title: t,
  seasons: [],
  episodes: [],
  variants: [],
  trailers: [],
  markers: [],
  ...over,
});
const page = (items: unknown[]) => ({ items, total: items.length, next_cursor: null });

const CURATOR = [
  'STAFF_BASIC',
  'REVIEW_MEDIA',
  'PUBLISH_MEDIA',
  'MANAGE_COLLECTIONS',
  'MANAGE_TV',
] as const;

const navNames = () =>
  within(nav())
    .getAllByRole('button')
    .map((b) => b.textContent);

describe('Cinema admin navigation', () => {
  it('shows the Media Curator the catalog and collections, but not transcoding', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([]))));
    openAdmin('cinema-catalog');
    await screen.findByRole('heading', { name: 'Cinema catalog' });
    expect(navNames()).toEqual([
      'Dashboard',
      'Review queues',
      'Cinema catalog',
      'Cinema collections',
    ]);
  });

  it('shows the Owner the transcoding section too', async () => {
    asStaff({ perms: [], owner: true });
    server.use(http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([]))));
    openAdmin('cinema-catalog');
    await screen.findByRole('heading', { name: 'Cinema catalog' });
    expect(navNames()).toEqual(
      expect.arrayContaining(['Cinema catalog', 'Cinema collections', 'Cinema transcoding']),
    );
  });
});

describe('Cinema catalog list', () => {
  it('lists titles, filters through the query and publishes with a visible conflict', async () => {
    asStaff({ perms: [...CURATOR] });
    const queries: string[] = [];
    let published = false;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', ({ request }) => {
        queries.push(new URL(request.url).search);
        return HttpResponse.json(page([title({ published })]));
      }),
      http.post('*/api/v1/admin/cinema/titles/10/publish', () =>
        HttpResponse.json(
          { code: 40900, message: 'Link a video before publishing.' },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-catalog');
    expect(await screen.findByText('Akira')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Delete Akira' })).toBeNull(); // DELETE_CONTENT is not a Curator power
    expect(screen.queryByRole('button', { name: /Merge Akira/ })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);

    await user.type(screen.getByRole('searchbox', { name: 'Search titles' }), 'aki');
    await waitFor(() => expect(queries.at(-1)).toContain('title=aki'));

    await user.click(screen.getByRole('button', { name: 'Publish Akira' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Link a video before publishing.');

    server.use(
      http.post('*/api/v1/admin/cinema/titles/10/publish', () =>
        HttpResponse.json(title({ published: true })),
      ),
    );
    published = true;
    await user.click(screen.getByRole('button', { name: 'Publish Akira' }));
    expect(await screen.findByRole('button', { name: 'Unpublish Akira' })).toBeInTheDocument();
    expect(await screen.findByText('Akira published.')).toBeInTheDocument();
  });

  it('deletes only after the title is typed, and merges into a chosen target (Owner)', async () => {
    asStaff({ perms: [], owner: true });
    const calls: string[] = [];
    server.use(
      http.get('*/api/v1/admin/cinema/titles', ({ request }) => {
        const q = new URL(request.url).searchParams.get('title');
        return HttpResponse.json(
          q === 'ak'
            ? page([title({ id: '11', title: 'Akira (2019)' })])
            : page([title(), title({ id: '12', title: 'Ghost' })]),
        );
      }),
      http.delete('*/api/v1/admin/cinema/titles/12', () => {
        calls.push('delete 12');
        return new HttpResponse(null, { status: 204 });
      }),
      http.post('*/api/v1/admin/cinema/titles/10/merge', async ({ request }) => {
        calls.push(`merge ${JSON.stringify(await readJson(request))}`);
        return HttpResponse.json(title({ id: '11' }));
      }),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await screen.findByText('Ghost');

    await user.click(screen.getByRole('button', { name: 'Delete Ghost' }));
    const dlg = await screen.findByRole('dialog', { name: 'Delete Ghost' });
    const confirm = within(dlg).getByRole('button', { name: 'Delete title' });
    expect(confirm).toBeDisabled();
    await user.type(within(dlg).getByRole('textbox'), 'Ghost');
    await user.click(confirm);
    await waitFor(() => expect(calls).toContain('delete 12'));

    await user.click(screen.getByRole('button', { name: 'Merge Akira into another title' }));
    const merge = await screen.findByRole('dialog', { name: /Merge Akira/ });
    await user.type(within(merge).getByRole('searchbox'), 'ak');
    await user.click(await within(merge).findByRole('button', { name: 'Keep Akira (2019)' }));
    await user.click(within(merge).getByRole('button', { name: 'Merge titles' }));
    await waitFor(() => expect(calls).toContain('merge {"target_id":"11"}'));
  });

  it('says plainly when the staff member may not open the catalog (403)', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () =>
        HttpResponse.json({ code: 40300, message: 'Missing permission' }, { status: 403 }),
      ),
    );
    openAdmin('cinema-catalog');
    expect(await screen.findByText('You do not have access')).toBeInTheDocument();
  });
});

describe('Title editor', () => {
  it('creates a draft title from the form', async () => {
    asStaff({ perms: [...CURATOR] });
    let body: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([]))),
      http.post('*/api/v1/admin/cinema/titles', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json(title({ id: '99', title: 'Paprika' }), { status: 201 });
      }),
      http.get('*/api/v1/admin/cinema/titles/99', () =>
        HttpResponse.json(detailOf(title({ id: '99', title: 'Paprika' }))),
      ),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'New title' }));
    await user.type(await screen.findByRole('textbox', { name: 'Title' }), 'Paprika');
    await user.type(screen.getByRole('textbox', { name: /^Genres/ }), 'Anime, Sci-fi');
    await user.type(screen.getByRole('spinbutton', { name: 'Year' }), '2006');
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Create draft' }));
    await waitFor(() =>
      expect(body).toMatchObject({
        kind: 'movie',
        title: 'Paprika',
        year: 2006,
        genres: ['Anime', 'Sci-fi'],
        age_restricted: false,
      }),
    );
    // the editor continues with the saved title and its tabs
    expect(await screen.findByRole('tab', { name: 'Artwork' })).toBeInTheDocument();
  });

  it('saving metadata keeps the existing artwork keys', async () => {
    asStaff({ perms: [...CURATOR] });
    let body: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      http.get('*/api/v1/admin/cinema/titles/10', () => HttpResponse.json(detailOf())),
      http.put('*/api/v1/admin/cinema/titles/10', async ({ request }) => {
        body = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(title());
      }),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    const tagline = await screen.findByRole('textbox', { name: 'Tagline' });
    await user.type(tagline, 'Katsuhiro');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(body.tagline).toBe('Katsuhiro'));
    expect(body.poster_key).toBe('cinema/10/poster.webp');
  });

  it('uploads artwork through the reservation route with a preview, then saves the key', async () => {
    asStaff({ perms: [...CURATOR] });
    const log: string[] = [];
    URL.createObjectURL = () => 'blob:preview';
    URL.revokeObjectURL = () => undefined;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      http.get('*/api/v1/admin/cinema/titles/10', () => HttpResponse.json(detailOf())),
      http.post('*/api/v1/admin/cinema/titles/10/artwork', async ({ request }) => {
        log.push(`reserve ${JSON.stringify(await readJson(request))}`);
        return HttpResponse.json({
          key: 'cinema/10/new.png',
          upload_url: 'https://storage.example.test/put/new',
          content_type: 'image/png',
          expires_in_secs: 600,
        });
      }),
      http.put('*/api/v1/admin/cinema/titles/10', async ({ request }) => {
        log.push(`save ${((await readJson(request)) as { poster_key: string }).poster_key}`);
        return HttpResponse.json(title());
      }),
    );
    putFile.mockImplementation(async (url: string) => {
      log.push(`put ${url}`);
    });
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Artwork' }));
    const file = new File(['x'], 'p.png', { type: 'image/png' });
    await user.upload(screen.getByLabelText('Choose poster file'), file);
    expect(await screen.findByAltText('Poster preview')).toHaveAttribute('src', 'blob:preview');
    await user.click(screen.getByRole('button', { name: 'Upload poster' }));
    await waitFor(() =>
      expect(log).toEqual([
        'reserve {"slot":"poster","content_type":"image/png"}',
        'put https://storage.example.test/put/new',
        'save cinema/10/new.png',
      ]),
    );
  });

  it('rejects a non-image file before uploading anything', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      http.get('*/api/v1/admin/cinema/titles/10', () => HttpResponse.json(detailOf())),
    );
    const user = userEvent.setup({ applyAccept: false });
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Artwork' }));
    await user.upload(
      screen.getByLabelText('Choose backdrop file'),
      new File(['x'], 'a.gif', { type: 'image/gif' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent('WebP, JPEG or PNG');
  });

  it('edits an episode inline and adds another', async () => {
    asStaff({ perms: [...CURATOR] });
    const series = title({ kind: 'series', title: 'Frieren' });
    const season = {
      id: '20',
      title_id: '10',
      number: 1,
      name: null,
      synopsis: null,
      poster_url: null,
      episode_count: 1,
    };
    const episode = {
      id: '30',
      title_id: '10',
      season_id: '20',
      season_number: 1,
      number: 1,
      absolute_number: null,
      title: 'The End of the Journey',
      synopsis: '',
      air_date: '2023-09-29',
      runtime_min: 24,
      still_url: null,
      playable: true,
      published: false,
      published_at_ms: null,
      asset_id: null,
    };
    const writes: string[] = [];
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([series]))),
      http.get('*/api/v1/admin/cinema/titles/10', () =>
        HttpResponse.json(detailOf(series, { seasons: [season], episodes: [episode] })),
      ),
      http.put('*/api/v1/admin/cinema/episodes/30', async ({ request }) => {
        writes.push(`put ${JSON.stringify(await readJson(request))}`);
        return HttpResponse.json(episode);
      }),
      http.post('*/api/v1/admin/cinema/seasons/20/episodes', async ({ request }) => {
        writes.push(`post ${JSON.stringify(await readJson(request))}`);
        return HttpResponse.json(episode, { status: 201 });
      }),
      http.post('*/api/v1/admin/cinema/episodes/30/publish', () =>
        HttpResponse.json({ code: 40900, message: 'Link a video first.' }, { status: 409 }),
      ),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Frieren' }));
    await user.click(await screen.findByRole('tab', { name: 'Seasons and episodes' }));
    expect(await screen.findByText('The End of the Journey')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Delete season/ })).toBeNull();

    await user.click(screen.getByRole('button', { name: 'Edit episode 1 of season 1' }));
    const t = await screen.findByRole('textbox', { name: 'Episode title' });
    await user.clear(t);
    await user.type(t, 'Departure');
    await user.click(screen.getByRole('button', { name: 'Save episode' }));
    await waitFor(() => expect(writes[0]).toContain('"title":"Departure"'));
    expect(writes[0]).toContain('"number":1');

    await user.click(screen.getByRole('button', { name: 'Add episode to season 1' }));
    await user.click(await screen.findByRole('button', { name: 'Add episode' }));
    await waitFor(() => expect(writes[1]).toContain('"number":2'));

    await user.click(screen.getByRole('button', { name: 'Publish episode 1 of season 1' }));
    expect(await screen.findByText('Link a video first.')).toBeInTheDocument();
  });

  it('validates and saves skip markers', async () => {
    asStaff({ perms: [...CURATOR] });
    let body: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () => HttpResponse.json(page([title()]))),
      http.get('*/api/v1/admin/cinema/titles/10', () =>
        HttpResponse.json(
          detailOf(title(), { markers: [{ kind: 'intro', start_ms: 5000, end_ms: 65000 }] }),
        ),
      ),
      http.put('*/api/v1/admin/cinema/titles/10/markers', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({
          markers: [{ kind: 'credits', start_ms: 7000000, end_ms: 7200000 }],
        });
      }),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Skip markers' }));
    const start = await screen.findByRole('spinbutton', { name: 'intro start' });
    expect(start).toHaveValue(5);
    expect(await axeViolations(container)).toEqual([]);

    await user.clear(screen.getByRole('spinbutton', { name: 'intro end' }));
    await user.type(screen.getByRole('spinbutton', { name: 'intro end' }), '2');
    expect(screen.getByText('The end must be after the start.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save markers' })).toBeDisabled();

    await user.clear(start);
    await user.clear(screen.getByRole('spinbutton', { name: 'intro end' }));
    await user.type(screen.getByRole('spinbutton', { name: 'credits start' }), '7000');
    await user.type(screen.getByRole('spinbutton', { name: 'credits end' }), '7200');
    await user.click(screen.getByRole('button', { name: 'Save markers' }));
    await waitFor(() =>
      expect(body).toEqual({ markers: [{ kind: 'credits', start_ms: 7000000, end_ms: 7200000 }] }),
    );
  });

  it('links an asset and reads its transcode status; re-run needs the permission', async () => {
    asStaff({ perms: [...CURATOR] });
    let linked: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () =>
        HttpResponse.json(page([title({ asset_id: '777' })])),
      ),
      http.get('*/api/v1/admin/cinema/titles/10', () =>
        HttpResponse.json(detailOf(title({ asset_id: '777' }))),
      ),
      http.put('*/api/v1/admin/cinema/titles/10/asset', async ({ request }) => {
        linked = await readJson(request);
        return HttpResponse.json(title({ asset_id: '888' }));
      }),
      http.get('*/api/v1/admin/transcode/assets/777', () =>
        HttpResponse.json({
          asset_id: '777',
          state: 'transcoding',
          profile_version: 3,
          jobs: [
            {
              id: '5',
              asset_id: '777',
              kind: 'ladder',
              state: 'running',
              priority: 1,
              attempt: 1,
              percent: 42,
              created_at_ms: 1_700_000_000_000,
            },
          ],
        }),
      ),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Video and transcoding' }));
    await user.click(await screen.findByRole('button', { name: /Show transcode status/ }));
    expect(await screen.findByText(/42%/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Re-run transcode' })).toBeNull();
    expect(screen.getByText(/Manage transcode permission/)).toBeInTheDocument();

    const field = screen.getByRole('textbox', { name: /Asset id for Akira/ });
    await user.clear(field);
    await user.type(field, '888');
    await user.click(screen.getByRole('button', { name: 'Link video' }));
    await waitFor(() => expect(linked).toEqual({ asset_id: '888' }));
  });

  it('shows a not-available state when the transcoding API does not exist yet', async () => {
    asStaff({ perms: [...CURATOR, 'MANAGE_TRANSCODE'] });
    server.use(
      http.get('*/api/v1/admin/cinema/titles', () =>
        HttpResponse.json(page([title({ asset_id: '777' })])),
      ),
      http.get('*/api/v1/admin/cinema/titles/10', () =>
        HttpResponse.json(detailOf(title({ asset_id: '777' }))),
      ),
      http.get(
        '*/api/v1/admin/transcode/assets/777',
        () => new HttpResponse(null, { status: 404 }),
      ),
    );
    const user = userEvent.setup();
    openAdmin('cinema-catalog');
    await user.click(await screen.findByRole('button', { name: 'Edit Akira' }));
    await user.click(await screen.findByRole('tab', { name: 'Video and transcoding' }));
    await user.click(await screen.findByRole('button', { name: /Show transcode status/ }));
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
  });
});

describe('Cinema collections', () => {
  const coll = (over: Record<string, unknown> = {}) => ({
    id: '50',
    slug: 'noir',
    name: 'Noir nights',
    eyebrow: null,
    blurb: null,
    display: 'row',
    surfaces: ['discover'],
    position: 1,
    published: true,
    item_count: 2,
    ...over,
  });

  it('reorders items and saves the new order', async () => {
    asStaff({ perms: [...CURATOR] });
    let order: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/collections', () => HttpResponse.json({ items: [coll()] })),
      http.get('*/api/v1/admin/cinema/collections/50', () =>
        HttpResponse.json({
          collection: coll(),
          items: [title({ id: '1', title: 'First' }), title({ id: '2', title: 'Second' })],
        }),
      ),
      http.put('*/api/v1/admin/cinema/collections/50/items', async ({ request }) => {
        order = await readJson(request);
        return HttpResponse.json({ collection: coll(), items: [] });
      }),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-collections');
    await user.click(await screen.findByRole('button', { name: 'Edit collection Noir nights' }));
    await screen.findByRole('list', { name: 'Ordered titles' });
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Move Second up' }));
    const items = within(screen.getByRole('list', { name: 'Ordered titles' })).getAllByRole(
      'listitem',
    );
    expect(items[0]).toHaveTextContent('Second');
    await user.click(screen.getByRole('button', { name: 'Save order' }));
    await waitFor(() => expect(order).toEqual({ title_ids: ['2', '1'] }));
  });

  it('is hidden from staff without the permission', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    openAdmin('dashboard');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(navNames()).not.toContain('Cinema collections');
  });
});

describe('Cinema transcoding', () => {
  it('lists workers with drain, tokens and queue jobs with logs', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
    let drained: unknown;
    server.use(
      http.get('*/api/v1/admin/transcode/jobs', () =>
        HttpResponse.json({
          items: [
            {
              id: '9',
              asset_id: '777',
              kind: 'ladder',
              state: 'failed',
              priority: 1,
              attempt: 3,
              error: 'ffmpeg exited 1',
              created_at_ms: 1_700_000_000_000,
            },
          ],
        }),
      ),
      http.get('*/api/v1/admin/transcode/jobs/9', () =>
        HttpResponse.json({
          id: '9',
          asset_id: '777',
          kind: 'ladder',
          state: 'failed',
          priority: 1,
          attempt: 3,
          created_at_ms: 1,
          log_tail: 'Conversion failed!',
        }),
      ),
      http.post('*/api/v1/admin/transcode/assets/777/retranscode', () =>
        HttpResponse.json({ asset_id: '777', state: 'transcoding', jobs: [] }),
      ),
      http.get('*/api/v1/admin/transcode/workers', () =>
        HttpResponse.json({
          items: [
            {
              id: 'w1',
              name: 'gpu-box',
              version: '1.2',
              capabilities: { nvenc: true },
              last_seen_ms: 1_700_000_000_000,
              online: true,
              drain: false,
              active_jobs: 1,
            },
          ],
        }),
      ),
      http.post('*/api/v1/admin/transcode/workers/w1/drain', async ({ request }) => {
        drained = await readJson(request);
        return HttpResponse.json({});
      }),
      http.get('*/api/v1/admin/transcode/tokens', () =>
        HttpResponse.json({
          items: [{ id: 't1', name: 'gpu-box', created_at_ms: 1_700_000_000_000 }],
        }),
      ),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-transcoding');
    expect(await screen.findByText('ffmpeg exited 1')).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);

    await user.click(screen.getByRole('button', { name: 'Logs for job 9' }));
    expect(await screen.findByText('Conversion failed!')).toBeInTheDocument();
    const logDlg = screen.getByRole('dialog', { name: /Logs for job 9/ });
    await user.click(within(logDlg).getByRole('button', { name: 'Done' }));

    await user.click(screen.getByRole('button', { name: 'Re-run job 9' }));
    const dlg = await screen.findByRole('dialog', { name: 'Re-run the transcode' });
    await user.type(within(dlg).getByRole('textbox'), 'again');
    await user.click(within(dlg).getByRole('button', { name: 'Queue transcode' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Re-run the transcode' })).toBeNull(),
    );

    await user.click(screen.getByRole('tab', { name: 'Workers' }));
    await user.click(await screen.findByRole('switch', { name: 'Drain gpu-box' }));
    await waitFor(() => expect(drained).toEqual({ drain: true }));

    await user.click(screen.getByRole('tab', { name: 'Worker tokens' }));
    expect(await screen.findByRole('button', { name: 'Revoke token gpu-box' })).toBeInTheDocument();
  });

  it('says it is not available yet on every tab when the routes are missing', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
    server.use(
      http.get('*/api/v1/admin/transcode/jobs', () => new HttpResponse(null, { status: 404 })),
      http.get('*/api/v1/admin/transcode/workers', () => new HttpResponse(null, { status: 404 })),
      http.get('*/api/v1/admin/transcode/tokens', () => new HttpResponse(null, { status: 404 })),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-transcoding');
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Workers' }));
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Worker tokens' }));
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows a one-time token secret after creating a token', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
    let created: unknown;
    server.use(
      http.get('*/api/v1/admin/transcode/jobs', () => HttpResponse.json({ items: [] })),
      http.get('*/api/v1/admin/transcode/tokens', () => HttpResponse.json({ items: [] })),
      http.post('*/api/v1/admin/transcode/tokens', async ({ request }) => {
        created = await readJson(request);
        return HttpResponse.json({
          token: { id: 't2', name: 'new-box', created_at_ms: 1 },
          secret: 'srw_secret_value',
        });
      }),
    );
    const user = userEvent.setup();
    openAdmin('cinema-transcoding');
    await user.click(await screen.findByRole('tab', { name: 'Worker tokens' }));
    await user.type(await screen.findByRole('textbox', { name: 'Token name' }), 'new-box');
    await user.click(screen.getByRole('button', { name: 'Create token' }));
    expect(await screen.findByText('srw_secret_value')).toBeInTheDocument();
    expect(created).toEqual({ name: 'new-box' });
  });
});

describe('helpers', () => {
  it('recovers the storage key from a signed cdn url', () => {
    expect(keyFromUrl('https://x.test/cdn/cinema/1/a%20b.webp?ex=1&sig=z')).toBe(
      'cinema/1/a b.webp',
    );
    expect(keyFromUrl('/cdn/library/1/a.webp')).toBeNull();
    expect(keyFromUrl(null)).toBeNull();
  });
  it('moves list items and clamps', () => {
    expect(move([1, 2, 3], 2, 0)).toEqual([3, 1, 2]);
    expect(move([1, 2, 3], 0, 5)).toEqual([1, 2, 3]);
  });
  it('validates marker windows', () => {
    const rows = {
      intro: { start: '10', end: '5' },
      recap: { start: '', end: '' },
      credits: { start: '100', end: '' },
    };
    expect(validateRows(rows)).toEqual({
      intro: 'The end must be after the start.',
      credits: 'Enter both a start and an end in seconds.',
    });
    expect(
      rowsToMarkers({
        ...rows,
        intro: { start: '1.5', end: '9' },
        credits: { start: '', end: '' },
      }),
    ).toEqual([{ kind: 'intro', start_ms: 1500, end_ms: 9000 }]);
  });
});
