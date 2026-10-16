import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { detail, event, item, preview, submission } from '../../review/__tests__/fixtures';
import type { SubmissionDetail } from '../../review/types';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const ME = '1';
const queue = (items = [submission('1', { title: 'Dungeon Tales' })], total = items.length) =>
  HttpResponse.json({ items, total, limit: 25, offset: 0 });

function useReviewApi(
  o: {
    detail?: () => SubmissionDetail;
    onPost?: (path: string, body: unknown) => SubmissionDetail | undefined;
  } = {},
) {
  const queues: string[] = [];
  let current = (o.detail ?? (() => detail()))();
  const act = (path: string) =>
    http.post(`*/api/v1/admin/review/submissions/:id/${path}`, async ({ request }) => {
      const body = await readJson(request);
      const next = o.onPost?.(path, body);
      if (next) current = next;
      return HttpResponse.json(current);
    });
  server.use(
    http.get('*/api/v1/admin/review/queue', ({ request }) => {
      queues.push(new URL(request.url).search);
      return queue();
    }),
    http.get('*/api/v1/admin/review/submissions/:id', () => HttpResponse.json(current)),
    http.get('*/api/v1/admin/review/submissions/:id/preview', () =>
      HttpResponse.json({ preview: preview() }),
    ),
    http.get('*/api/v1/admin/review/submissions/:id/library/chapters/:c/pages', () =>
      HttpResponse.json({
        pages: [0, 1].map((i) => ({ index: i, width: 100, height: 150, url: `/cdn/p${i}.webp` })),
      }),
    ),
    act('claim'),
    act('release'),
    act('approve'),
    act('request-changes'),
    act('reject'),
    act('comments'),
    act('publish'),
    http.put('*/api/v1/admin/review/submissions/:id/items/:item', async ({ request }) => {
      const body = (await readJson(request)) as {
        state: 'approved' | 'rejected' | 'pending';
        note?: string;
      };
      const next = o.onPost?.('item', body);
      if (next) current = next;
      return HttpResponse.json(current);
    }),
  );
  return queues;
}

const lease = () => ({ claimed_by: ME, claim_expires_at_ms: Date.now() + 20 * 60_000 });

describe('review queue', () => {
  beforeEach(() => asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] }));

  it('lists the queue, filters on the server and passes axe', async () => {
    const queues = useReviewApi();
    server.use(
      http.get('*/api/v1/admin/review/queue', ({ request }) => {
        queues.push(new URL(request.url).search);
        return queue([
          submission('1', { title: 'Dungeon Tales' }),
          submission('2', {
            title: 'Other',
            kind: 'library.items',
            ...lease(),
            state: 'in_review',
          }),
        ]);
      }),
    );
    const user = userEvent.setup();
    const { container } = openAdmin('review');
    expect(await screen.findByRole('heading', { name: 'Review queues' })).toBeInTheDocument();
    const table = await screen.findByRole('table', { name: 'Review queue' });
    expect(within(table).getByText('Dungeon Tales')).toBeInTheDocument();
    expect(within(table).getByText('New chapters')).toBeInTheDocument();
    expect(within(table).getByText(/^You until/)).toBeInTheDocument();
    expect(within(table).getByText('Waiting for review')).toBeInTheDocument();
    expect(queues[0]).toContain('sort=submitted');
    expect(queues[0]).toContain('order=asc');
    expect(queues[0]).not.toContain('state=');
    expect(await axeViolations(container)).toEqual([]);

    await user.type(screen.getByRole('textbox', { name: 'Title contains' }), 'dung');
    await user.click(screen.getByRole('button', { name: 'Filter' }));
    await waitFor(() => expect(queues.at(-1)).toContain('q=dung'));
  });

  it('shows an empty state and pages through results', async () => {
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/admin/review/queue', ({ request }) => {
        const u = new URL(request.url);
        seen.push(u.search);
        const off = Number(u.searchParams.get('offset') ?? 0);
        const items = Array.from({ length: off === 0 ? 25 : 5 }, (_, i) =>
          submission(String(off + i + 1), { title: `S${off + i + 1}` }),
        );
        return HttpResponse.json({ items, total: 30, limit: 25, offset: off });
      }),
    );
    const user = userEvent.setup();
    openAdmin('review');
    await screen.findByText('S1');
    expect(screen.getByText('1 to 25 of 30')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await screen.findByText('S26');
    expect(screen.getByText('26 to 30 of 30')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
    expect(seen.at(-1)).toContain('offset=25');
  });

  it('opens the submission a #review-queue card links to, and its reject dialog (G-68)', async () => {
    useReviewApi({
      detail: () => detail({ id: '7', title: 'Linked series', state: 'submitted' }),
      onPost: (path) =>
        path === 'claim'
          ? detail({ id: '7', title: 'Linked series', state: 'in_review', ...lease() })
          : undefined,
    });
    openAdmin('review?submission=7&reject=1');
    expect(await screen.findByRole('heading', { name: 'Linked series' })).toBeInTheDocument();
    // The reviewer holds the claim by now and the reject dialog is open, asking for a reason.
    const dialog = await screen.findByRole('dialog', { name: 'Reject this submission' });
    expect(within(dialog).getByRole('button', { name: 'Reject' })).toBeDisabled();
  });

  it("opens the request-changes dialog a card's Request changes button links to (H-04)", async () => {
    useReviewApi({
      detail: () => detail({ id: '7', title: 'Linked series', state: 'submitted' }),
      onPost: (path) =>
        path === 'claim'
          ? detail({ id: '7', title: 'Linked series', state: 'in_review', ...lease() })
          : undefined,
    });
    openAdmin('review?submission=7&changes=1');
    expect(await screen.findByRole('heading', { name: 'Linked series' })).toBeInTheDocument();
    const dialog = await screen.findByRole('dialog', { name: 'Request changes' });
    expect(within(dialog).getByRole('button', { name: 'Send back' })).toBeDisabled();
  });

  it('says so when nothing waits', async () => {
    server.use(http.get('*/api/v1/admin/review/queue', () => queue([], 0)));
    openAdmin('review');
    expect(await screen.findByText('Nothing to review')).toBeInTheDocument();
  });

  it('explains a failed load with a retry', async () => {
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ message: 'boom' }, { status: 500 }),
      ),
    );
    openAdmin('review');
    expect(await screen.findByText('Could not load this section')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });
});

describe('review page', () => {
  beforeEach(() => asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] }));

  async function open(user: ReturnType<typeof userEvent.setup>) {
    const view = openAdmin('review');
    await user.click(await screen.findByRole('button', { name: 'Open Dungeon Tales' }));
    await screen.findByRole('heading', { name: 'Submission 1' });
    return view;
  }

  it('shows metadata, items, the timeline and passes axe', async () => {
    useReviewApi();
    const user = userEvent.setup();
    const { container } = await open(user);
    expect(await screen.findByText('Kui')).toBeInTheDocument();
    expect(screen.getByText('Delicious.')).toBeInTheDocument();
    expect(screen.getByText(/First submission/)).toBeInTheDocument();
    const items = screen.getByRole('table', { name: 'Items under review' });
    expect(within(items).getByText('Chapter 1')).toBeInTheDocument();
    expect(within(items).getAllByText('12 pages')).toHaveLength(2);
    expect(screen.getByRole('list', { name: 'Timeline' })).toBeInTheDocument();
    // Not claimed: the decision bar is locked and says why.
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Reject' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Request changes' })).toBeDisabled();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('claims, then approve publishes', async () => {
    const posts: string[] = [];
    useReviewApi({
      onPost: (path) => {
        posts.push(path);
        if (path === 'claim') return detail({ state: 'in_review', ...lease() });
        if (path === 'approve')
          return detail({ state: 'published', decided_by: ME }, [
            item(1, { state: 'published' }),
            item(2, { state: 'published' }),
          ]);
        return undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    await user.click(screen.getByRole('button', { name: 'Claim' }));
    expect(
      await screen.findByText('You claimed this submission for 30 minutes.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Release' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Approve' }));
    const dialog = await screen.findByRole('dialog', { name: 'Approve this submission' });
    expect(
      within(dialog).getByText(/2 of 2 items will be approved and published/),
    ).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Approve and publish' }));
    await waitFor(() => expect(posts).toEqual(['claim', 'approve']));
    expect((await screen.findAllByText('Published')).length).toBeGreaterThan(0);
  });

  it('request changes needs a reason and sends it', async () => {
    let body: unknown;
    useReviewApi({
      detail: () => detail({ state: 'in_review', ...lease() }),
      onPost: (path, b) => {
        if (path === 'request-changes') {
          body = b;
          return detail({ state: 'changes_requested', decision_reason: 'Add a cover' });
        }
        return undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    await user.click(screen.getByRole('button', { name: 'Request changes' }));
    const dialog = await screen.findByRole('dialog', { name: 'Request changes' });
    const send = within(dialog).getByRole('button', { name: 'Send back' });
    expect(send).toBeDisabled();
    await user.type(within(dialog).getByRole('textbox'), 'Add a cover');
    await user.click(send);
    await waitFor(() => expect(body).toEqual({ reason: 'Add a cover' }));
    expect(await screen.findByText('Changes requested', { selector: 'span' })).toBeInTheDocument();
  });

  it('reject needs a reason template and joins the note', async () => {
    let body: unknown;
    useReviewApi({
      detail: () => detail({ state: 'in_review', ...lease() }),
      onPost: (path, b) => {
        if (path === 'reject') {
          body = b;
          return detail({ state: 'rejected' });
        }
        return undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    await user.click(screen.getByRole('button', { name: 'Reject' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reject this submission' });
    const confirm = within(dialog).getByRole('button', { name: 'Reject' });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByRole('combobox', { name: 'Reason' }));
    await user.click(await screen.findByRole('option', { name: 'Poor image quality' }));
    expect(confirm).toBeEnabled();
    await user.type(within(dialog).getByRole('textbox'), 'pages 3 to 8 are blurry');
    await user.click(confirm);
    await waitFor(() =>
      expect(body).toEqual({ reason: 'Poor image quality: pages 3 to 8 are blurry' }),
    );
  });

  it('decides per item and refuses an item rejection without a note', async () => {
    const calls: unknown[] = [];
    useReviewApi({
      detail: () => detail({ state: 'in_review', ...lease() }),
      onPost: (path, b) => {
        if (path !== 'item') return undefined;
        calls.push(b);
        return calls.length === 1
          ? detail({ state: 'in_review', ...lease() }, [item(1, { state: 'approved' }), item(2)])
          : detail({ state: 'in_review', ...lease() }, [
              item(1, { state: 'approved' }),
              item(2, { state: 'rejected', note: 'blurry' }),
            ]);
      },
    });
    const user = userEvent.setup();
    await open(user);
    await user.click(screen.getByRole('button', { name: 'Approve Chapter 1' }));
    await waitFor(() => expect(calls).toEqual([{ state: 'approved' }]));
    await user.click(await screen.findByRole('button', { name: 'Reject Chapter 2' }));
    const reject = screen.getByRole('button', { name: 'Reject item' });
    expect(reject).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: /Why is Chapter 2 rejected/ }), 'blurry');
    await user.click(reject);
    await waitFor(() => expect(calls.at(-1)).toEqual({ state: 'rejected', note: 'blurry' }));
    expect(await screen.findByText('blurry')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reset Chapter 2' })).toBeInTheDocument();
  });

  it('sends public and internal notes', async () => {
    const bodies: unknown[] = [];
    useReviewApi({
      onPost: (path, b) => {
        if (path === 'comments') {
          bodies.push(b);
          return detail({}, undefined, [
            event(1, 'created'),
            event(3, 'comment', { actor_id: ME, body: 'looks fine', internal: true }),
          ]);
        }
        return undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    await user.type(screen.getByRole('textbox', { name: 'Add a note' }), 'looks fine');
    await user.click(screen.getByRole('checkbox', { name: /Internal note/ }));
    await user.click(screen.getByRole('button', { name: 'Add note' }));
    await waitFor(() => expect(bodies).toEqual([{ body: 'looks fine', internal: true }]));
    const timeline = await screen.findByRole('list', { name: 'Timeline' });
    expect(within(timeline).getByText('Internal')).toBeInTheDocument();
    expect(within(timeline).getByText('looks fine')).toBeInTheDocument();
  });

  it('shows what changed between two submits', async () => {
    useReviewApi({
      detail: () =>
        detail({ state: 'submitted' }, undefined, [
          event(1, 'created'),
          event(2, 'submitted', { data: { snapshot: preview({ synopsis: 'Old text' }, 3) } }),
          event(3, 'changes_requested', { actor_id: '5', body: 'More pages' }),
          event(4, 'resubmitted', { data: { snapshot: preview({ synopsis: 'New text' }, 12) } }),
        ]),
    });
    const user = userEvent.setup();
    await open(user);
    const changes = await screen.findByRole('list', { name: 'Changes since the previous submit' });
    expect(within(changes).getByText('Synopsis')).toBeInTheDocument();
    expect(within(changes).getByText('Old text')).toBeInTheDocument();
    expect(within(changes).getByText('New text')).toBeInTheDocument();
    expect(within(changes).getAllByText('3 pages').length).toBeGreaterThan(0);
  });

  it('a lease held by someone else blocks decisions; curators can take over', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING', 'PUBLISH_READING'] });
    const posts: unknown[] = [];
    useReviewApi({
      detail: () =>
        detail({
          state: 'in_review',
          claimed_by: '55',
          claim_expires_at_ms: Date.now() + 10 * 60_000,
        }),
      onPost: (path, b) => {
        if (path === 'claim') {
          posts.push(b);
          return detail({ state: 'in_review', ...lease() });
        }
        return undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    expect(screen.getByText(/Account 55 is reviewing this until/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Claim' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Take over the claim' }));
    const dialog = await screen.findByRole('dialog', { name: 'Take over the claim' });
    await user.click(within(dialog).getByRole('button', { name: 'Take over' }));
    await waitFor(() => expect(posts).toEqual([{ takeover: true }]));
    expect(await screen.findByRole('button', { name: 'Release' })).toBeInTheDocument();
  });

  it('a reviewer without the publish permission cannot take over', async () => {
    useReviewApi({
      detail: () =>
        detail({
          state: 'in_review',
          claimed_by: '55',
          claim_expires_at_ms: Date.now() + 10 * 60_000,
        }),
    });
    const user = userEvent.setup();
    await open(user);
    expect(screen.queryByRole('button', { name: 'Take over the claim' })).toBeNull();
  });

  it('offers a retry when publishing failed after approval', async () => {
    const posts: string[] = [];
    useReviewApi({
      detail: () =>
        detail(
          { state: 'approved' },
          [item(1, { state: 'approved' })],
          [
            event(1, 'created'),
            event(5, 'publish_failed', {
              actor_id: ME,
              body: 'Chapter 1 has no pages',
              internal: true,
            }),
          ],
        ),
      onPost: (path) => {
        posts.push(path);
        return path === 'publish' ? detail({ state: 'published' }) : undefined;
      },
    });
    const user = userEvent.setup();
    await open(user);
    expect((await screen.findAllByText('Publishing failed')).length).toBeGreaterThan(0);
    await user.click(screen.getByRole('button', { name: 'Try publishing again' }));
    await waitFor(() => expect(posts).toEqual(['publish']));
  });

  it('opens the page previews of a chapter', async () => {
    useReviewApi();
    const user = userEvent.setup();
    await open(user);
    await user.click(await screen.findByRole('button', { name: 'View pages of Chapter 1' }));
    const dialog = await screen.findByRole('dialog', { name: 'Chapter 1' });
    expect(within(dialog).getAllByRole('img')).toHaveLength(2);
  });

  it('shows an API refusal in the status area', async () => {
    useReviewApi();
    server.use(
      http.post('*/api/v1/admin/review/submissions/:id/claim', () =>
        HttpResponse.json(
          { code: 50000, message: 'Another reviewer holds the claim.' },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    await open(user);
    await user.click(screen.getByRole('button', { name: 'Claim' }));
    expect(await screen.findByText('Another reviewer holds the claim.')).toBeInTheDocument();
  });
});
