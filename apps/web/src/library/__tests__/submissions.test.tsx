import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { detail, event, item, preview, submission } from '../../review/__tests__/fixtures';
import type { DraftChapter, SubmissionDetail, Upload } from '../../review/types';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { renderAt, useLibraryApi } from './harness';

const draftChapter = (n: number, o: Partial<DraftChapter> = {}): DraftChapter => ({
  id: `c${n}`,
  series_id: '900',
  edition_id: 'e1',
  number: n,
  title: null,
  page_count: 0,
  format: 'images',
  release_group: null,
  published: false,
  ...o,
});

const upload = (o: Partial<Upload> = {}): Upload => ({
  id: 'u1',
  chapter_id: 'c1',
  series_id: '900',
  filename: 'ch1.cbz',
  kind: 'archive',
  size_bytes: 10,
  status: 'done',
  error: null,
  warnings: [],
  page_count: 12,
  metadata: {},
  created_at_ms: 1,
  updated_at_ms: 2,
  finished_at_ms: 3,
  ...o,
});

function useSubmissionsApi(
  o: {
    list?: ReturnType<typeof submission>[];
    detail?: SubmissionDetail;
    chapters?: DraftChapter[];
  } = {},
) {
  const log = {
    lists: [] as string[],
    posts: [] as { path: string; body: unknown }[],
  };
  let current = o.detail ?? detail({ state: 'draft' }, []);
  let chapters = o.chapters ?? [];
  const rec = async (path: string, request: Request) => {
    let body: unknown;
    try {
      body = await request.clone().json();
    } catch {
      body = undefined;
    }
    log.posts.push({ path, body });
  };
  useLibraryApi();
  server.use(
    http.get('*/api/v1/submissions', ({ request }) => {
      log.lists.push(new URL(request.url).search);
      const items = o.list ?? [];
      return HttpResponse.json({ items, total: items.length, limit: 50, offset: 0 });
    }),
    http.post('*/api/v1/submissions', async ({ request }) => {
      await rec('create', request);
      current = detail({ state: 'draft', title: 'Dungeon Tales' }, []);
      return HttpResponse.json(current, { status: 201 });
    }),
    http.get('*/api/v1/submissions/:id', () => HttpResponse.json(current)),
    http.patch('*/api/v1/submissions/:id', async ({ request }) => {
      await rec('patch', request);
      return HttpResponse.json(current);
    }),
    http.get('*/api/v1/submissions/:id/preview', () => HttpResponse.json({ preview: preview() })),
    http.get('*/api/v1/submissions/:id/library/chapters', () => HttpResponse.json({ chapters })),
    http.post('*/api/v1/submissions/:id/library/chapters', async ({ request }) => {
      await rec('chapter', request);
      const c = draftChapter(chapters.length + 1);
      chapters = [...chapters, c];
      current = { ...current, items: [...current.items, item(chapters.length, { ref_id: c.id })] };
      return HttpResponse.json(c, { status: 201 });
    }),
    http.delete('*/api/v1/submissions/:id/library/chapters/:c', async ({ params }) => {
      log.posts.push({ path: `delete:${String(params.c)}`, body: undefined });
      chapters = chapters.filter((c) => c.id !== params.c);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/submissions/:id/library/chapters/:c/uploads', () =>
      HttpResponse.json({ uploads: [] }),
    ),
    http.post('*/api/v1/submissions/:id/comments', async ({ request }) => {
      await rec('comment', request);
      return HttpResponse.json(current);
    }),
    http.post('*/api/v1/submissions/:id/submit', async ({ request }) => {
      await rec('submit', request);
      current = detail({ state: 'submitted' }, current.items);
      return HttpResponse.json(current);
    }),
    http.post('*/api/v1/submissions/:id/withdraw', async ({ request }) => {
      await rec('withdraw', request);
      current = detail({ state: 'withdrawn' }, current.items);
      return HttpResponse.json(current);
    }),
  );
  return log;
}

describe('Library submissions list', () => {
  it('lists my submissions with their review state and reviewer notes, and passes axe', async () => {
    useSubmissionsApi({
      list: [
        submission('1', { title: 'Dungeon Tales', state: 'draft' }),
        submission('2', {
          title: 'Needs work',
          state: 'changes_requested',
          decision_reason: 'Add a cover',
        }),
        submission('3', {
          title: 'Nope',
          state: 'rejected',
          kind: 'library.items',
          decision_reason: 'Duplicate',
        }),
        submission('4', { title: 'Live', state: 'published' }),
      ],
    });
    const { container } = renderAt('/library/submissions');
    expect(await screen.findByRole('heading', { name: 'Submissions' })).toBeInTheDocument();
    const list = await screen.findByRole('list', { name: 'My submissions' });
    expect(within(list).getByRole('link', { name: 'Dungeon Tales' })).toHaveAttribute(
      'href',
      '/library/submissions/1',
    );
    expect(within(list).getByText('Draft')).toBeInTheDocument();
    expect(within(list).getByText('Changes requested')).toBeInTheDocument();
    expect(within(list).getByText(/Add a cover/)).toBeInTheDocument();
    expect(within(list).getByText(/Duplicate/)).toBeInTheDocument();
    expect(within(list).getByText('Published')).toBeInTheDocument();
    // The sidebar entry is the active one.
    expect(screen.getByRole('link', { name: 'Submissions' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('filters by state on the server and shows an empty state', async () => {
    const log = useSubmissionsApi({ list: [] });
    const user = userEvent.setup();
    renderAt('/library/submissions');
    expect(await screen.findByText('No submissions yet')).toBeInTheDocument();
    expect(log.lists[0]).toContain('sort=updated');
    await user.click(screen.getByRole('combobox', { name: 'Filter submissions' }));
    await user.click(await screen.findByRole('option', { name: 'Changes requested' }));
    await waitFor(() => expect(log.lists.at(-1)).toContain('state=changes_requested'));
  });

  it('starts a new series draft and opens it', async () => {
    const log = useSubmissionsApi({ list: [] });
    const user = userEvent.setup();
    renderAt('/library/submissions');
    await user.click(await screen.findByRole('button', { name: 'New submission' }));
    const form = await screen.findByRole('form', { name: 'New submission' });
    const start = within(form).getByRole('button', { name: 'Start the draft' });
    expect(start).toBeDisabled();
    await user.type(within(form).getByRole('textbox', { name: 'Title' }), 'Dungeon Tales');
    await user.type(within(form).getByRole('textbox', { name: /Authors/ }), 'Kui, Someone');
    await user.click(start);
    expect(await screen.findByRole('heading', { name: 'Dungeon Tales' })).toBeInTheDocument();
    expect(log.posts[0]).toEqual({
      path: 'create',
      body: {
        kind: 'library.series',
        payload: {
          kind: 'manga',
          title: 'Dungeon Tales',
          authors: ['Kui', 'Someone'],
          genres: [],
          synopsis: '',
          age_restricted: false,
        },
      },
    });
  });

  it('opens the new-chapters form prefilled from ?series=', async () => {
    const log = useSubmissionsApi({ list: [] });
    const user = userEvent.setup();
    renderAt('/library/submissions?series=123');
    const form = await screen.findByRole('form', { name: 'New submission' });
    expect(within(form).getByRole('textbox', { name: /Series id/ })).toHaveValue('123');
    await user.click(within(form).getByRole('button', { name: 'Start the draft' }));
    await waitFor(() =>
      expect(log.posts[0]?.body).toMatchObject({
        kind: 'library.items',
        payload: { series_id: '123' },
      }),
    );
  });
});

describe('Library submission page', () => {
  it('adds a chapter, uploads its file, and submits', async () => {
    const log = useSubmissionsApi({ chapters: [] });
    const status: Upload['status'] = 'queued';
    server.use(
      http.post('*/api/v1/submissions/:id/library/chapters/:c/uploads', async ({ request }) => {
        log.posts.push({ path: 'begin', body: await request.clone().json() });
        return HttpResponse.json(
          {
            upload: upload({ status: 'awaiting_upload' }),
            upload_url: 'http://storage.test/put/u1',
            upload_method: 'PUT',
            expires_in_secs: 600,
            max_bytes: 1000,
          },
          { status: 201 },
        );
      }),
      http.put('http://storage.test/put/u1', async ({ request }) => {
        log.posts.push({ path: 'put', body: (await request.arrayBuffer()).byteLength });
        return new HttpResponse(null, { status: 200 });
      }),
      http.post('*/api/v1/submissions/:id/library/uploads/:u/complete', () => {
        log.posts.push({ path: 'complete', body: undefined });
        return HttpResponse.json(upload({ status: 'queued' }), { status: 202 });
      }),
      http.get('*/api/v1/submissions/:id/library/chapters/:c/uploads', () =>
        HttpResponse.json({
          uploads: log.posts.some((p) => p.path === 'complete') ? [upload({ status })] : [],
        }),
      ),
    );
    const user = userEvent.setup();
    const { container } = renderAt('/library/submissions/1');
    expect(await screen.findByRole('heading', { name: 'Submission 1' })).toBeInTheDocument();
    expect(await screen.findByText(/No chapters yet/)).toBeInTheDocument();

    // Add chapter 1.
    const add = screen.getByRole('form', { name: 'Add a chapter' });
    await user.type(within(add).getByRole('textbox', { name: 'Chapter number' }), '1');
    await user.click(within(add).getByRole('button', { name: 'Add chapter' }));
    const table = await screen.findByRole('table', { name: 'Chapters of this submission' });
    expect(within(table).getByText('Chapter 1')).toBeInTheDocument();
    expect(within(table).getByText('No pages yet')).toBeInTheDocument();
    expect(log.posts.find((p) => p.path === 'chapter')?.body).toEqual({
      number: 1,
      title: null,
      format: 'images',
    });
    expect(await axeViolations(container)).toEqual([]);

    // Upload the file: begin, PUT, complete, then the conversion finishes.
    const file = new File(['PK-bytes'], 'ch1.cbz', { type: 'application/zip' });
    await user.upload(screen.getByLabelText('Choose a file for chapter 1'), file);
    await waitFor(() =>
      expect(log.posts.map((p) => p.path)).toEqual(
        expect.arrayContaining(['begin', 'put', 'complete']),
      ),
    );
    expect(log.posts.find((p) => p.path === 'begin')?.body).toEqual({
      filename: 'ch1.cbz',
      size_bytes: 8,
    });
    expect(await screen.findByText(/Queued for conversion/)).toBeInTheDocument();

    // Submit for review.
    await user.click(screen.getByRole('button', { name: 'Submit for review' }));
    await waitFor(() => expect(log.posts.some((p) => p.path === 'submit')).toBe(true));
    expect(await screen.findByText('Waiting for review', { selector: 'span' })).toBeInTheDocument();
    // The editing controls are gone.
    expect(screen.queryByRole('form', { name: 'Add a chapter' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Submit for review' })).toBeNull();
  });

  it('lists what is not ready when the server refuses the submit', async () => {
    useSubmissionsApi({ chapters: [draftChapter(1)] });
    server.use(
      http.post('*/api/v1/submissions/:id/submit', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              problems: {
                '0': {
                  _errors: [
                    { code: 'SUBMISSION_NOT_READY', message: 'Chapter 1 has no pages yet' },
                  ],
                },
                '1': { _errors: [{ code: 'SUBMISSION_NOT_READY', message: 'Add a synopsis' }] },
              },
            },
          },
          { status: 400 },
        ),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/submissions/1');
    await user.click(await screen.findByRole('button', { name: 'Submit for review' }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText('Chapter 1 has no pages yet')).toBeInTheDocument();
    expect(within(alert).getByText('Add a synopsis')).toBeInTheDocument();
  });

  it('shows a failed conversion with its reason', async () => {
    useSubmissionsApi({ chapters: [draftChapter(1)] });
    server.use(
      http.get('*/api/v1/submissions/:id/library/chapters/:c/uploads', () =>
        HttpResponse.json({
          uploads: [
            upload({
              status: 'failed',
              error: 'page 4 is not an image',
              warnings: ['skipped Thumbs.db'],
            }),
          ],
        }),
      ),
    );
    renderAt('/library/submissions/1');
    expect(await screen.findByText('page 4 is not an image')).toBeInTheDocument();
    expect(screen.getByText('skipped Thumbs.db')).toBeInTheDocument();
  });

  it('removes a chapter and edits the details', async () => {
    const log = useSubmissionsApi({
      chapters: [draftChapter(1), draftChapter(2)],
      detail: detail({ state: 'draft' }, [item(1, { ref_id: 'c1' }), item(2, { ref_id: 'c2' })]),
    });
    const user = userEvent.setup();
    renderAt('/library/submissions/1');
    await user.click(await screen.findByRole('button', { name: 'Remove Chapter 2' }));
    await waitFor(() => expect(log.posts.some((p) => p.path === 'delete:c2')).toBe(true));
    await waitFor(() => expect(screen.queryByText('Chapter 2')).toBeNull());
    const form = screen.getByRole('form', { name: 'Submission details' });
    const save = within(form).getByRole('button', { name: 'Save details' });
    expect(save).toBeDisabled();
    await user.type(
      within(form).getByRole('textbox', { name: /Note for the reviewers/ }),
      'Scans by me',
    );
    await user.click(save);
    await waitFor(() =>
      expect(log.posts.find((p) => p.path === 'patch')?.body).toEqual({
        title: 'Submission 1',
        summary: 'Scans by me',
      }),
    );
  });

  it('shows the reviewer note and per-item notes, and resubmits', async () => {
    const log = useSubmissionsApi({
      chapters: [draftChapter(1, { page_count: 3 }), draftChapter(2, { page_count: 9 })],
      detail: detail(
        { state: 'changes_requested', decision_reason: 'Chapter 1 is missing pages' },
        [
          item(1, { ref_id: 'c1', state: 'rejected', note: 'only 3 pages' }),
          item(2, { ref_id: 'c2', state: 'approved' }),
        ],
        [
          event(1, 'created'),
          event(2, 'submitted', { data: { snapshot: preview() } }),
          event(3, 'changes_requested', { actor_id: '5', body: 'Chapter 1 is missing pages' }),
        ],
      ),
    });
    const user = userEvent.setup();
    const { container } = renderAt('/library/submissions/1');
    const banner = await screen.findByText('A reviewer asked for changes');
    expect(banner.parentElement).toHaveTextContent('Chapter 1 is missing pages');
    const table = await screen.findByRole('table', { name: 'Chapters of this submission' });
    expect(within(table).getByText('only 3 pages')).toBeInTheDocument();
    expect(within(table).getByText('Rejected')).toBeInTheDocument();
    expect(within(table).getByText('Approved')).toBeInTheDocument();
    // The timeline shows the reviewer's note without any internal detail.
    expect(
      within(screen.getByRole('list', { name: 'Timeline' })).getByText(
        'A reviewer · ' +
          new Date(1_700_000_000_000 + 3 * 60_000).toLocaleString(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
          }),
      ),
    ).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Send for review again' }));
    await waitFor(() => expect(log.posts.some((p) => p.path === 'submit')).toBe(true));
  });

  it('withdraws after confirming', async () => {
    const log = useSubmissionsApi({
      detail: detail({ state: 'submitted' }, [item(1, { ref_id: 'c1' })]),
      chapters: [draftChapter(1, { page_count: 3 })],
    });
    const user = userEvent.setup();
    renderAt('/library/submissions/1');
    await user.click(await screen.findByRole('button', { name: 'Withdraw' }));
    const dialog = await screen.findByRole('dialog', { name: 'Withdraw this submission' });
    expect(log.posts.some((p) => p.path === 'withdraw')).toBe(false);
    await user.click(within(dialog).getByRole('button', { name: 'Withdraw' }));
    await waitFor(() => expect(log.posts.some((p) => p.path === 'withdraw')).toBe(true));
    expect((await screen.findAllByText('Withdrawn')).length).toBeGreaterThan(0);
    expect(screen.queryByRole('button', { name: 'Withdraw' })).toBeNull();
  });

  it('refreshes when the gateway reports an update', async () => {
    let state: SubmissionDetail = detail({ state: 'submitted' }, [item(1, { ref_id: 'c1' })]);
    useSubmissionsApi({ chapters: [draftChapter(1, { page_count: 3 })] });
    server.use(http.get('*/api/v1/submissions/:id', () => HttpResponse.json(state)));
    renderAt('/library/submissions/1');
    await screen.findByText('Waiting for review', { selector: 'span' });
    state = detail({ state: 'published' }, [item(1, { ref_id: 'c1', state: 'published' })]);
    act(() => emitDispatch('SR_SUBMISSION_UPDATE', { type: 'published', submission_id: '1' }));
    expect(await screen.findByText('This is live in the library. Thank you!')).toBeInTheDocument();
  });

  it('says so when the submission is not found', async () => {
    useSubmissionsApi();
    server.use(
      http.get('*/api/v1/submissions/:id', () =>
        HttpResponse.json({ message: 'Unknown submission' }, { status: 404 }),
      ),
    );
    renderAt('/library/submissions/9');
    expect(await screen.findByText('Submission not found')).toBeInTheDocument();
  });
});
