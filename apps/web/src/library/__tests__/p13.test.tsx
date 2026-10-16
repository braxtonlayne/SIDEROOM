// Phase 13 Library web work: comments (series tab and reader drawer), related and contributors tabs, Patron
// downloads, "Share to Lounge", the reading heartbeat, the bell count, and the history pause.
// LIB-SOC-003/005/007/009, LIB-SER-002/004/005, LIB-RDR-014/016/017.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { useReaderPrefsStore } from '../prefs';
import { localProgress, setProgressApi } from '../progress';
import { useStore } from '../../store';
import { A, DONE, manifest, renderAt, useLibraryApi } from './harness';

const comment = (id: string, o: Record<string, unknown> = {}) => ({
  id,
  series_id: '1',
  chapter_id: '0',
  parent_id: null,
  author_id: '9',
  author_name: 'Ana',
  body: `Comment ${id}`,
  spoiler: false,
  like_count: 0,
  reply_count: 0,
  liked: false,
  mine: false,
  created_at_ms: Date.now() - 60_000,
  edited_at_ms: null,
  ...o,
});

beforeEach(() => {
  localStorage.clear();
  useReaderPrefsStore.setState({ global: {}, series: {} });
  setProgressApi(localProgress);
});

describe('comments tab (LIB-SOC-003)', () => {
  it('lists comments, hides spoilers, posts, replies and likes', async () => {
    useLibraryApi();
    const posted: unknown[] = [];
    let liked = 0;
    server.use(
      http.get('*/api/v1/library/series/1/comments', () =>
        HttpResponse.json({
          items: [
            comment('10', { reply_count: 1 }),
            comment('11', { spoiler: true, body: 'The butler did it', author_name: 'Bo' }),
          ],
          next_before: null,
          total: 3,
        }),
      ),
      http.get('*/api/v1/library/comments/10/replies', () =>
        HttpResponse.json({
          items: [comment('12', { parent_id: '10', body: 'A reply', author_name: 'Cy' })],
          next_before: null,
          total: 1,
        }),
      ),
      http.post('*/api/v1/library/series/1/comments', async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json(comment('13'), { status: 201 });
      }),
      http.put('*/api/v1/library/comments/10/like', () => {
        liked += 1;
        return HttpResponse.json({ count: 1 });
      }),
    );
    const user = userEvent.setup();
    const { container } = renderAt('/library/s/1');
    await user.click(await screen.findByRole('tab', { name: 'Comments' }));
    expect(await screen.findByText('3 comments')).toBeInTheDocument();
    expect(screen.getByText('Comment 10')).toBeInTheDocument();
    // The spoiler is hidden until asked for.
    expect(screen.queryByText('The butler did it')).not.toBeInTheDocument();
    const second = screen.getByText('Bo').closest('li')!;
    await user.click(within(second).getByRole('button', { name: 'Show' }));
    expect(screen.getByText('The butler did it')).toBeInTheDocument();
    // Replies load on demand.
    await user.click(screen.getByRole('button', { name: 'Show 1 reply' }));
    expect(await screen.findByText('A reply')).toBeInTheDocument();
    // Likes.
    const first = screen.getByText('Comment 10').closest('li')!;
    await user.click(within(first).getAllByRole('button', { name: /like/ })[0]!);
    await waitFor(() => expect(liked).toBe(1));
    // Posting sends the body and the spoiler flag.
    await user.type(screen.getByLabelText('Add a comment'), 'Loved it');
    await user.click(screen.getByRole('checkbox', { name: 'Contains spoilers' }));
    await user.click(screen.getByRole('button', { name: 'Post' }));
    await waitFor(() => expect(posted).toEqual([{ body: 'Loved it', spoiler: true }]));
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows the empty state', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/series/1/comments', () =>
        HttpResponse.json({ items: [], next_before: null, total: 0 }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('tab', { name: 'Comments' }));
    expect(await screen.findByText('No comments yet')).toBeInTheDocument();
  });
});

describe('related and contributors tabs (LIB-SOC-005, LIB-SER-005)', () => {
  it('shows related series with the reason, and contributors with groups', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/series/1/related', () =>
        HttpResponse.json({ items: [{ score: 4, reasons: ['tags', 'coread'], series: DONE }] }),
      ),
      http.get('*/api/v1/library/series/1/contributors', () =>
        HttpResponse.json({
          members: [{ user_id: '5', name: 'Uploader Una', chapters: 12 }],
          groups: [{ name: 'Scan Team', chapters: 30 }],
        }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('tab', { name: 'Related' }));
    expect(await screen.findByText('Monster')).toBeInTheDocument();
    expect(
      screen.getByText('Similar genres and tags · Read by people you know'),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Contributors' }));
    expect(await screen.findByText('Uploader Una')).toBeInTheDocument();
    expect(screen.getByText('12 chapters')).toBeInTheDocument();
    expect(screen.getByText('Scan Team')).toBeInTheDocument();
  });
});

describe('downloads and sharing (LIB-SER-002/004, LIB-RDR-016)', () => {
  it('shows download buttons only to Patrons and saves the file', async () => {
    useLibraryApi();
    let asked = 0;
    server.use(
      http.get('*/api/v1/library/me/downloads', () =>
        HttpResponse.json({ entitled: true, per_day: 5, left: 4 }),
      ),
      http.post('*/api/v1/library/chapters/:id/download', () => {
        asked += 1;
        return HttpResponse.json({
          format: 'cbz',
          filename: 'Frieren-ch-3.cbz',
          url: '/cdn/library/exports/3/x.cbz',
          size_bytes: 100,
          expires_at: 9_999_999_999,
        });
      }),
    );
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => undefined);
    const user = userEvent.setup();
    renderAt('/library/s/1');
    const row = (await screen.findByText(/Chapter 3/, { selector: '.lib-chapter-title' })).closest(
      'li',
    )!;
    await user.click(await within(row).findByRole('button', { name: 'Download chapter 3' }));
    await waitFor(() => expect(asked).toBe(1));
    await waitFor(() => expect(click).toHaveBeenCalled());
    click.mockRestore();
  });

  it('hides them without the perk, and shares the series link into a picked channel', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/me/downloads', () =>
        HttpResponse.json({ entitled: false, per_day: 0, left: 0 }),
      ),
    );
    const sent: [string, string][] = [];
    useStore.setState({
      guilds: {},
      guildOrder: [],
      channels: { d1: { id: 'd1', type: 1, recipients: [{ id: '9', username: 'ana' }] } as never },
      channelIdsByGuild: { '@me': ['d1'] },
      sendMessage: (async (channelId: string, content: string) => {
        sent.push([channelId, content]);
        return {} as never;
      }) as never,
    });
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await screen.findByText(/Chapter 3/, { selector: '.lib-chapter-title' });
    expect(screen.queryByRole('button', { name: /Download chapter/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Share to Lounge' }));
    const dialog = await screen.findByRole('dialog', { name: 'Share to Lounge' });
    await user.type(within(dialog).getByLabelText('Add a note (optional)'), 'Try this');
    await user.click(within(dialog).getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]![0]).toBe('d1');
    const [first, second] = sent[0]![1].split('\n');
    expect(first).toBe('Try this');
    // D-226: the Lounge draws this token as the Library rich card.
    expect(second).toBe('<library:1>');
  });
});

describe('reader comments, presence and the bell (LIB-RDR-014/017, LIB-SOC-007)', () => {
  it('opens the chapter comments drawer from the top bar and the end card, and beats the presence', async () => {
    useLibraryApi();
    let beats = 0;
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(manifest(1))),
      http.put('*/api/v1/library/chapters/c1/presence', () => {
        beats += 1;
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/library/chapters/c1/comments', () =>
        HttpResponse.json({
          items: [comment('20', { chapter_id: 'c1', body: 'Great chapter' })],
          next_before: null,
          total: 2,
        }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await waitFor(() => expect(beats).toBeGreaterThan(0));
    await user.click(await screen.findByRole('button', { name: 'Comments (2)' }));
    expect(await screen.findByText('Great chapter')).toBeInTheDocument();
  });

  it('shows the bell count and clears it when the bell is opened', async () => {
    useLibraryApi();
    let seen = 0;
    server.use(
      http.get('*/api/v1/library/me/notifications', () => HttpResponse.json({ unread: 4 })),
      http.post('*/api/v1/library/me/notifications/seen', () => {
        seen += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library');
    const bell = await screen.findByRole('button', { name: 'Notifications, 4 new' });
    await user.click(bell);
    await waitFor(() => expect(seen).toBe(1));
    expect(await screen.findByRole('button', { name: 'Notifications' })).toBeInTheDocument();
  });
});

describe('Because you read (LIB-SOC-005)', () => {
  it('shows a row per seed on the home page', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/recommendations', () =>
        HttpResponse.json({
          rows: [
            {
              seed_id: '1',
              seed_title: 'Frieren',
              items: [{ score: 3, reasons: ['tags'], series: DONE }],
            },
          ],
        }),
      ),
    );
    renderAt('/library');
    expect(
      await screen.findByRole('region', { name: 'Because you read Frieren' }),
    ).toBeInTheDocument();
  });
});

describe('history pause (LIB-SOC-009)', () => {
  it('toggles activity.paused and groups the timeline by day', async () => {
    useLibraryApi();
    let patch: unknown = null;
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 1, settings: { activity: { paused: false } } }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({ version: 2, settings: { activity: { paused: true } } });
      }),
    );
    localProgress.save({
      chapterId: 'c1',
      seriesId: A.id,
      seriesTitle: A.title,
      coverUrl: A.cover_url,
      chapterNumber: 1,
      pageCount: 3,
      pageIndex: 1,
      percent: 0.5,
      completed: false,
    });
    const user = userEvent.setup();
    renderAt('/library/history');
    expect(await screen.findByRole('region', { name: 'Today' })).toBeInTheDocument();
    await user.click(await screen.findByRole('switch', { name: 'Pause history' }));
    await waitFor(() => expect(patch).toEqual({ settings: { activity: { paused: true } } }));
  });
});
