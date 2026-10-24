import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { detail, event, submission } from '../../review/__tests__/fixtures';
import { MemoryLedger, UploadManager } from '../../review/uploadManager';
import { http, HttpResponse, server } from '../../test/server';
import type { Draft, DraftItem, DraftVideo } from '../submit/api';
import { cinemaUploadApi, setCinemaUploadManager } from '../submit/uploads';
import { renderAt, SHOW, useCinemaApi } from './harness';

const video = (o: Partial<DraftVideo> = {}): DraftVideo => ({
  asset_id: '500',
  state: 'published',
  error: null,
  filename: 'Night Shift - s01e01.mkv',
  size_bytes: 10,
  duration_ms: 1_380_000,
  has_proxy: true,
  playable: true,
  ...o,
});

const row = (n: number, v: DraftVideo | null): DraftItem => ({
  target_kind: 'episode',
  target_id: `e${n}`,
  season_number: 1,
  number: n,
  label: `S01E0${n} Episode ${n}`,
  video: v,
});

const draft = (items: DraftItem[], o: Partial<Draft['draft']> = {}): Draft => ({
  draft: {
    new_title: true,
    title: {
      id: '900',
      kind: 'series',
      title: 'Night Shift',
      original_title: null,
      alt_titles: [],
      year: 2024,
      runtime_min: null,
      content_rating: null,
      synopsis: 'A night job.',
      tagline: null,
      genres: ['drama'],
      tags: [],
      languages: [],
      subtitle_languages: [],
      cast: [],
      directors: [],
      writers: [],
      studios: [],
      crew: [],
      age_restricted: false,
      poster_url: null,
      backdrop_url: null,
      external_ids: {},
    },
    seasons: [{ id: 's1', number: 1, name: null }],
    episodes: items.map((i) => ({
      id: i.target_id,
      season_number: i.season_number ?? 1,
      number: i.number ?? 1,
      title: `Episode ${i.number}`,
      synopsis: '',
      air_date: null,
      runtime_min: null,
      published: false,
    })),
    published_episodes: 0,
    ...o,
  },
  items,
  pending: [],
});

interface Log {
  lists: string[];
  calls: { method: string; path: string; body: unknown }[];
}

function useSubmitApi(
  o: {
    list?: ReturnType<typeof submission>[];
    draft?: Draft;
    state?: Parameters<typeof detail>[0];
    submitProblems?: string[];
  } = {},
): Log {
  const log: Log = { lists: [], calls: [] };
  useCinemaApi();
  const rec = async (method: string, path: string, request: Request) =>
    log.calls.push({
      method,
      path: path.replace('/api/v1', ''),
      body: await request
        .clone()
        .json()
        .catch(() => undefined),
    });
  let current = detail(
    { state: 'draft', kind: 'cinema.title', title: 'Night Shift', ...o.state },
    [],
    [event(1, 'created')],
  );
  current = { ...current, kind_label: 'Cinema title' };
  server.use(
    http.get('*/api/v1/submissions', ({ request }) => {
      log.lists.push(new URL(request.url).search);
      const items = o.list ?? [];
      return HttpResponse.json({ items, total: items.length, limit: 50, offset: 0 });
    }),
    http.post('*/api/v1/submissions', async ({ request }) => {
      await rec('POST', '/submissions', request);
      return HttpResponse.json(current, { status: 201 });
    }),
    http.get('*/api/v1/submissions/:id', () => HttpResponse.json(current)),
    http.get('*/api/v1/submissions/:id/cinema/draft', () =>
      HttpResponse.json(o.draft ?? draft([row(1, video()), row(2, null)])),
    ),
    http.put('*/api/v1/submissions/:id/cinema/title', async ({ request }) => {
      await rec('PUT', '/title', request);
      return HttpResponse.json({ ...(o.draft ?? draft([])).draft.title, title: 'Night Shift' });
    }),
    http.post('*/api/v1/submissions/:id/cinema/episodes', async ({ request }) => {
      await rec('POST', '/episodes', request);
      return HttpResponse.json({ id: 'e9' }, { status: 201 });
    }),
    http.put('*/api/v1/submissions/:id/cinema/episodes/:e', async ({ request, params }) => {
      await rec('PUT', `/episodes/${String(params.e)}`, request);
      return HttpResponse.json({ id: params.e });
    }),
    http.delete('*/api/v1/submissions/:id/cinema/episodes/:e', async ({ request, params }) => {
      await rec('DELETE', `/episodes/${String(params.e)}`, request);
      return new HttpResponse(null, { status: 204 });
    }),
    http.post('*/api/v1/submissions/:id/comments', () => HttpResponse.json(current)),
    http.post('*/api/v1/submissions/:id/withdraw', async ({ request }) => {
      await rec('POST', '/withdraw', request);
      current = detail({ state: 'withdrawn', kind: 'cinema.title' }, []);
      return HttpResponse.json(current);
    }),
    http.post('*/api/v1/submissions/:id/submit', async ({ request }) => {
      await rec('POST', '/submit', request);
      if (o.submitProblems)
        return HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              problems: Object.fromEntries(
                o.submitProblems.map((m, i) => [
                  String(i),
                  { _errors: [{ code: 'X', message: m }] },
                ]),
              ),
            },
          },
          { status: 400 },
        );
      current = detail({ state: 'submitted', kind: 'cinema.title' }, []);
      return HttpResponse.json(current);
    }),
  );
  return log;
}

afterEach(() => setCinemaUploadManager(null));

describe('Cinema > Submit list', () => {
  it('lists my video submissions with their review state, and passes axe', async () => {
    const log = useSubmitApi({
      list: [
        submission('1', { title: 'Night Shift', state: 'draft', kind: 'cinema.title' }),
        submission('2', {
          title: 'New episodes for Old Show',
          state: 'changes_requested',
          kind: 'cinema.episodes',
          decision_reason: 'Episode 3 is cut off',
        }),
        submission('3', {
          title: 'Nope',
          state: 'rejected',
          kind: 'cinema.title',
          decision_reason: 'Duplicate',
        }),
      ],
    });
    const { container } = renderAt('/cinema/submit');
    expect(await screen.findByRole('heading', { name: 'Submit a video' })).toBeInTheDocument();
    const list = await screen.findByRole('list', { name: 'My video submissions' });
    expect(within(list).getByRole('link', { name: 'Night Shift' })).toHaveAttribute(
      'href',
      '/cinema/submit/1',
    );
    expect(within(list).getByText('Draft')).toBeInTheDocument();
    expect(within(list).getByText(/Episode 3 is cut off/)).toBeInTheDocument();
    expect(within(list).getByText(/New episodes/, { selector: 'p' })).toBeInTheDocument();
    expect(within(list).getByText(/Duplicate/)).toBeInTheDocument();
    // Only the Cinema kinds are asked for, and the nav shows where we are.
    expect(log.lists[0]).toContain('kind=cinema.title%2Ccinema.episodes');
    expect(screen.getAllByRole('link', { name: 'Submit' })[0]).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows an empty state and filters on the server', async () => {
    const log = useSubmitApi({ list: [] });
    const user = userEvent.setup();
    renderAt('/cinema/submit');
    expect(await screen.findByText('No video submissions yet')).toBeInTheDocument();
    await user.click(screen.getByRole('combobox', { name: 'Filter my submissions' }));
    await user.click(await screen.findByRole('option', { name: 'Changes requested' }));
    await waitFor(() => expect(log.lists.at(-1)).toContain('state=changes_requested'));
  });

  it('starts a series draft and opens it', async () => {
    const log = useSubmitApi({ list: [] });
    const user = userEvent.setup();
    renderAt('/cinema/submit');
    await user.click(await screen.findByRole('button', { name: 'New submission' }));
    const form = await screen.findByRole('form', { name: 'New submission' });
    const start = within(form).getByRole('button', { name: 'Start the draft' });
    expect(start).toBeDisabled();
    await user.type(within(form).getByRole('textbox', { name: 'Title' }), 'Night Shift');
    await user.type(within(form).getByRole('textbox', { name: 'Year' }), '2024');
    await user.click(start);
    expect(await screen.findByRole('heading', { name: 'Night Shift' })).toBeInTheDocument();
    expect(log.calls[0]).toEqual({
      method: 'POST',
      path: '/submissions',
      body: {
        kind: 'cinema.title',
        payload: { kind: 'series', title: 'Night Shift', year: 2024, synopsis: '' },
      },
    });
  });

  it('adds episodes to a series that is already in the catalog', async () => {
    const log = useSubmitApi({ list: [] });
    server.use(
      http.get('*/api/v1/cinema/search', () =>
        HttpResponse.json({ titles: [SHOW], people: [], genres: [], tags: [] }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/cinema/submit');
    await user.click(await screen.findByRole('button', { name: 'New submission' }));
    const form = await screen.findByRole('form', { name: 'New submission' });
    await user.click(within(form).getByRole('combobox', { name: 'What are you sending?' }));
    await user.click(
      await screen.findByRole('option', { name: /New episodes for a series that is already here/ }),
    );
    expect(within(form).getByRole('button', { name: 'Start the draft' })).toBeDisabled();
    await user.type(within(form).getByRole('textbox', { name: /Find the series/ }), 'Seve');
    await user.click(
      await within(form).findByRole('button', { name: 'Add episodes to Severance' }),
    );
    expect(await within(form).findByText('Adding episodes to Severance.')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: 'Start the draft' }));
    await waitFor(() =>
      expect(log.calls[0]?.body).toEqual({
        kind: 'cinema.episodes',
        payload: { title_id: '2', season: 1, episodes: [] },
      }),
    );
  });
});

describe('Cinema > Submit draft', () => {
  it('shows every video with its processing state and submits', async () => {
    const log = useSubmitApi({
      draft: draft([
        row(1, video()),
        row(2, video({ asset_id: '501', state: 'transcoding', playable: false })),
        row(
          3,
          video({
            asset_id: '502',
            state: 'failed',
            playable: false,
            error: 'The file is not a video.',
          }),
        ),
        row(4, null),
      ]),
    });
    const user = userEvent.setup();
    const { container } = renderAt('/cinema/submit/1');
    expect(await screen.findByRole('heading', { name: 'Night Shift' })).toBeInTheDocument();
    const list = await screen.findByRole('list', { name: 'Videos of this submission' });
    const rows = within(list).getAllByRole('listitem');
    expect(within(rows[0]!).getByText('Ready')).toBeInTheDocument();
    expect(within(rows[0]!).getByText(/23 min/)).toBeInTheDocument();
    expect(within(rows[1]!).getByText('Preparing for playback')).toBeInTheDocument();
    expect(within(rows[2]!).getByText('Failed')).toBeInTheDocument();
    expect(within(rows[2]!).getByText(/The file is not a video/)).toBeInTheDocument();
    expect(within(rows[3]!).getByText('No video yet')).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Send for review' }));
    await waitFor(() => expect(log.calls.some((c) => c.path === '/submit')).toBe(true));
    expect((await screen.findAllByText('Waiting for review')).length).toBeGreaterThan(0);
  });

  it('shows what stops the submission when a video is not ready', async () => {
    useSubmitApi({ submitProblems: ['S01E02 Episode 2 is still being processed'] });
    const user = userEvent.setup();
    renderAt('/cinema/submit/1');
    await user.click(await screen.findByRole('button', { name: 'Send for review' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'S01E02 Episode 2 is still being processed',
    );
  });

  it('edits the title, an episode and removes one', async () => {
    const log = useSubmitApi();
    const user = userEvent.setup();
    renderAt('/cinema/submit/1');
    const form = await screen.findByRole('form', { name: 'Title details' });
    await user.clear(within(form).getByRole('textbox', { name: 'Synopsis' }));
    await user.type(within(form).getByRole('textbox', { name: 'Synopsis' }), 'Nights, mostly.');
    await user.click(within(form).getByRole('button', { name: 'Save details' }));
    await waitFor(() => expect(log.calls.some((c) => c.path === '/title')).toBe(true));
    const sent = log.calls.find((c) => c.path === '/title')!.body as Record<string, unknown>;
    expect(sent.synopsis).toBe('Nights, mostly.');
    expect(sent.kind).toBe('series');
    expect(sent.year).toBe(2024);
    // The artwork is never sent back: the server keeps what it holds.
    expect(sent).not.toHaveProperty('poster_url');
    expect(sent).not.toHaveProperty('id');

    await user.click(screen.getByRole('button', { name: 'Edit S01E01 Episode 1' }));
    const ep = await screen.findByRole('form', { name: 'Details of episode 1' });
    await user.type(within(ep).getByRole('textbox', { name: 'Episode title' }), '!');
    await user.click(within(ep).getByRole('button', { name: 'Save episode' }));
    await waitFor(() => expect(log.calls.some((c) => c.path === '/episodes/e1')).toBe(true));

    await user.click(screen.getByRole('button', { name: 'Remove S01E02 Episode 2' }));
    await waitFor(() =>
      expect(log.calls.some((c) => c.method === 'DELETE' && c.path === '/episodes/e2')).toBe(true),
    );
  });

  it('reads the file names into seasons and episodes, and uploads through the manager', async () => {
    useSubmitApi({ draft: draft([]) });
    const begun: { name: string; target: string }[] = [];
    const manager = new UploadManager(
      {
        ...cinemaUploadApi,
        async begin(_sub, target, b) {
          begun.push({ name: b.filename, target });
          return {
            upload: { id: `a${begun.length}` },
            multipart: {
              part_size: 4,
              part_count: 1,
              parts: [{ part_number: 1, url: 'https://s.test/p1' }],
            },
          } as never;
        },
        complete: async () => ({}) as never,
      },
      new MemoryLedger(),
      { put: async () => undefined },
    );
    setCinemaUploadManager(manager);
    server.use(
      http.post('*/api/v1/submissions/naming/parse', async ({ request }) => {
        const b = (await request.json()) as { section: string; names: string[] };
        return HttpResponse.json({
          items: b.names.map((name) => {
            const m = /s(\d+)e(\d+)/i.exec(name);
            return {
              name,
              library: null,
              cinema: {
                role: 'video',
                title: 'Night Shift',
                year: null,
                season: m ? Number(m[1]) : null,
                episodes: m ? [Number(m[2])] : [],
                episode_title: null,
                variant: null,
                language: null,
                warnings: [],
              },
            };
          }),
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/cinema/submit/1');
    const input = await screen.findByLabelText('Video files');
    const files = [
      new File(['0123'], 'Night Shift - s01e01 - Pilot.mkv', { type: 'video/x-matroska' }),
      new File(['0123'], 'Night Shift - s01e02.mkv', { type: 'video/x-matroska' }),
      new File(['0123'], 'holiday.mkv', { type: 'video/x-matroska' }),
      new File(['x'], 'notes.txt', { type: 'text/plain' }),
    ];
    fireEvent.change(input, { target: { files } });
    const proposal = await screen.findByRole('region', { name: 'How your files were read' });
    // Text files are left out; a name with no numbering asks for them.
    expect(within(proposal).queryByText(/notes.txt/)).toBeNull();
    expect(
      within(proposal).getByRole('textbox', { name: 'Season of Night Shift - s01e01 - Pilot.mkv' }),
    ).toHaveValue('1');
    expect(
      within(proposal).getByRole('textbox', { name: 'Episode of Night Shift - s01e02.mkv' }),
    ).toHaveValue('2');
    expect(within(proposal).getByText(/Could not read the season and episode/)).toBeInTheDocument();
    const go = within(proposal).getByRole('button', { name: /Upload 2 videos/ });
    // The unplaced file is fixed by hand.
    await user.type(within(proposal).getByRole('textbox', { name: 'Season of holiday.mkv' }), '2');
    await user.type(within(proposal).getByRole('textbox', { name: 'Episode of holiday.mkv' }), '5');
    await user.click(await within(proposal).findByRole('button', { name: /Upload 3 videos/ }));
    expect(go).toBeDefined();
    await waitFor(() => expect(begun).toHaveLength(3));
    expect(begun.map((b) => b.target).sort()).toEqual(['se:1:1', 'se:1:2', 'se:2:5']);
    // The dock shows the uploads.
    expect(await screen.findByRole('region', { name: 'Uploads' })).toBeInTheDocument();
  });

  it('does not send two files to the same episode', async () => {
    useSubmitApi({ draft: draft([]) });
    server.use(
      http.post('*/api/v1/submissions/naming/parse', async ({ request }) => {
        const b = (await request.json()) as { names: string[] };
        return HttpResponse.json({
          items: b.names.map((name) => ({
            name,
            library: null,
            cinema: {
              role: 'video',
              title: 'X',
              year: null,
              season: 1,
              episodes: [1],
              episode_title: null,
              variant: null,
              language: null,
              warnings: [],
            },
          })),
        });
      }),
    );
    renderAt('/cinema/submit/1');
    const input = await screen.findByLabelText('Video files');
    fireEvent.change(input, {
      target: {
        files: [new File(['a'], 'X - s01e01.mkv'), new File(['b'], 'X - s01e01 (copy).mkv')],
      },
    });
    expect(
      await screen.findByText(/Two files are placed at season 1, episode 1/),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Upload 2 videos/ })).toBeDisabled();
  });

  it('takes one file for a film and sends it for the server to place', async () => {
    useSubmitApi({
      draft: draft([], {
        title: { ...draft([]).draft.title, kind: 'movie', title: 'Quiet Film' },
      }),
    });
    const begun: { name: string; target: string }[] = [];
    setCinemaUploadManager(
      new UploadManager(
        {
          ...cinemaUploadApi,
          async begin(_s, target, b) {
            begun.push({ name: b.filename, target });
            return {
              upload: { id: 'a1' },
              multipart: {
                part_size: 4,
                part_count: 1,
                parts: [{ part_number: 1, url: 'https://s.test/p1' }],
              },
            } as never;
          },
          complete: async () => ({}) as never,
        },
        new MemoryLedger(),
        { put: async () => undefined },
      ),
    );
    server.use(
      http.post('*/api/v1/submissions/naming/parse', async ({ request }) => {
        const b = (await request.json()) as { names: string[] };
        return HttpResponse.json({
          items: b.names.map((name) => ({
            name,
            library: null,
            cinema: {
              role: 'video',
              title: 'Quiet Film',
              year: 2024,
              season: null,
              episodes: [],
              episode_title: null,
              variant: null,
              language: null,
              warnings: [],
            },
          })),
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/cinema/submit/1');
    const input = await screen.findByLabelText('Video files');
    fireEvent.change(input, {
      target: {
        files: [
          new File(['a'], 'Quiet Film (2024).mkv'),
          new File(['b'], 'Quiet Film (2024) (1).mkv'),
        ],
      },
    });
    expect(await screen.findByText(/A film has one video/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Upload 1 video/ }));
    await waitFor(() => expect(begun).toEqual([{ name: 'Quiet Film (2024).mkv', target: 'auto' }]));
  });

  it('withdraws after a confirmation', async () => {
    const log = useSubmitApi();
    const user = userEvent.setup();
    renderAt('/cinema/submit/1');
    await user.click(await screen.findByRole('button', { name: 'Withdraw' }));
    const dialog = await screen.findByRole('dialog', { name: /Withdraw this submission/ });
    await user.click(within(dialog).getByRole('button', { name: 'Withdraw' }));
    await waitFor(() => expect(log.calls.some((c) => c.path === '/withdraw')).toBe(true));
  });
});

describe('the Cinema upload manager over the Cinema routes', () => {
  it('begins through /cinema/files, then resumes after a refresh from the ledger and sends only the missing part', async () => {
    const sent: string[] = [];
    const calls: string[] = [];
    server.use(
      http.post('*/api/v1/submissions/7/cinema/files', async ({ request }) => {
        calls.push(`files ${JSON.stringify(await request.json())}`);
        return HttpResponse.json(
          {
            item: row(1, null),
            created: true,
            asset_id: '500',
            part_size: 4,
            part_count: 3,
            parts: [1, 2, 3].map((n) => ({ part_number: n, url: `https://s.test/p${n}` })),
            expires_in_secs: 21600,
            warnings: [],
            resumable: true,
          },
          { status: 201 },
        );
      }),
      http.get('*/api/v1/submissions/7/cinema/uploads/500/parts', () => {
        calls.push('parts');
        return HttpResponse.json({
          part_size: 4,
          part_count: 3,
          received: [
            { part_number: 1, size: 4 },
            { part_number: 2, size: 4 },
          ],
          missing: [{ part_number: 3, url: 'https://s.test/p3b' }],
          expires_in_secs: 21600,
        });
      }),
      http.post('*/api/v1/submissions/7/cinema/uploads/500/complete', () => {
        calls.push('complete');
        return HttpResponse.json(video({ state: 'uploaded' }), { status: 202 });
      }),
    );
    const ledger = new MemoryLedger();
    const file = new File(['0123456789'], 'Night Shift - s01e01.mkv');
    // First page: parts 1 and 2 go out, then the page "closes" (the third put never answers).
    const first = new UploadManager(cinemaUploadApi, ledger, {
      put: async (url) => {
        sent.push(url);
        if (url.endsWith('p3')) await new Promise(() => undefined);
      },
      retries: 0,
    });
    void first.enqueue('7', 'se:1:1', file).catch(() => undefined);
    await waitFor(() => expect(sent).toContain('https://s.test/p2'));
    await waitFor(async () => expect((await ledger.all())[0]).toMatchObject({ uploadId: '500' }));
    expect(calls[0]).toContain('"season":1');
    expect(calls[0]).toContain('"episode":1');

    // After a refresh a new manager reads the same ledger and asks the server what arrived.
    sent.length = 0;
    const second = new UploadManager(cinemaUploadApi, ledger, {
      put: async (url) => {
        sent.push(url);
      },
    });
    await second.resumeAll();
    await waitFor(() => expect(calls).toContain('complete'));
    expect(calls).toContain('parts');
    expect(sent).toEqual(['https://s.test/p3b']);
    expect(await ledger.all()).toEqual([]);
  });
});
