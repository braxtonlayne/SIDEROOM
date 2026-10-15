import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { renderAt, useLibraryApi } from '../../library/__tests__/harness';
import { http, HttpResponse, server } from '../../test/server';
import type { ContentRequest } from '../types';

const request = (id: string, o: Partial<ContentRequest> = {}): ContentRequest => ({
  id,
  section: 'library',
  title: `Wanted ${id}`,
  creator: null,
  year: null,
  link: null,
  note: '',
  status: 'open',
  vote_count: 1,
  voted: false,
  submission_id: null,
  created_at_ms: Date.now() - 3600_000,
  claimed_by: null,
  claim_note: null,
  claimed_by_me: false,
  ...o,
});

function useRequestsApi(initial: ContentRequest[]) {
  let rows = initial;
  const log = { lists: [] as string[], calls: [] as { path: string; body: unknown }[] };
  const replace = (id: string, f: (r: ContentRequest) => ContentRequest) => {
    rows = rows.map((r) => (r.id === id ? f(r) : r));
    return rows.find((r) => r.id === id)!;
  };
  useLibraryApi();
  server.use(
    http.get('*/api/v1/requests', ({ request: req }) => {
      log.lists.push(new URL(req.url).search);
      return HttpResponse.json({ items: rows, total: rows.length, limit: 20, offset: 0 });
    }),
    http.post('*/api/v1/requests', async ({ request: req }) => {
      const body = await req.json();
      log.calls.push({ path: 'create', body });
      const r = request('99', { title: (body as { title: string }).title, voted: true });
      rows = [r, ...rows];
      return HttpResponse.json(r, { status: 201 });
    }),
    http.put('*/api/v1/requests/:id/vote', ({ params }) => {
      log.calls.push({ path: `vote:${String(params.id)}`, body: undefined });
      return HttpResponse.json(
        replace(String(params.id), (r) => ({ ...r, voted: true, vote_count: r.vote_count + 1 })),
      );
    }),
    http.delete('*/api/v1/requests/:id/vote', ({ params }) => {
      log.calls.push({ path: `unvote:${String(params.id)}`, body: undefined });
      return HttpResponse.json(
        replace(String(params.id), (r) => ({ ...r, voted: false, vote_count: r.vote_count - 1 })),
      );
    }),
    http.put('*/api/v1/requests/:id/claim', async ({ params, request: req }) => {
      const body = (await req.json()) as { note: string };
      log.calls.push({ path: `claim:${String(params.id)}`, body });
      return HttpResponse.json(
        replace(String(params.id), (r) => ({
          ...r,
          claimed_by: '1',
          claimed_by_me: true,
          claim_note: body.note || null,
        })),
      );
    }),
    http.delete('*/api/v1/requests/:id/claim', ({ params }) => {
      log.calls.push({ path: `release:${String(params.id)}`, body: undefined });
      return HttpResponse.json(
        replace(String(params.id), (r) => ({
          ...r,
          claimed_by: null,
          claimed_by_me: false,
          claim_note: null,
        })),
      );
    }),
  );
  return log;
}

describe('Requests board', () => {
  it('lists open requests by votes with the sort and status filters, and passes axe', async () => {
    const log = useRequestsApi([
      request('1', { title: 'Dungeon Meshi', vote_count: 5, creator: 'Ryoko Kui', year: 2014 }),
      request('2', { title: 'Frieren', claimed_by: '55', claim_note: 'vol 3 scans' }),
    ]);
    const { container } = renderAt('/library/requests');
    expect(await screen.findByRole('heading', { name: 'Requests' })).toBeInTheDocument();
    const list = await screen.findByRole('list', { name: 'Requests' });
    expect(within(list).getByText(/Dungeon Meshi/)).toBeInTheDocument();
    expect(within(list).getByText('By Ryoko Kui', { exact: false })).toBeInTheDocument();
    expect(within(list).getByText('5 votes')).toBeInTheDocument();
    expect(within(list).getByText(/Someone is working on this/)).toBeInTheDocument();
    expect(within(list).getByText(/vol 3 scans/)).toBeInTheDocument();
    expect(log.lists[0]).toContain('section=library');
    expect(log.lists[0]).toContain('sort=votes');
    expect(log.lists[0]).toContain('status=open');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows an empty state', async () => {
    useRequestsApi([]);
    renderAt('/library/requests');
    expect(await screen.findByText('Nothing requested yet')).toBeInTheDocument();
  });

  it('votes and takes the vote back', async () => {
    const user = userEvent.setup();
    const log = useRequestsApi([request('1', { title: 'Dungeon Meshi' })]);
    renderAt('/library/requests');
    await user.click(await screen.findByRole('button', { name: 'Vote for Dungeon Meshi' }));
    const back = await screen.findByRole('button', {
      name: 'Take back your vote for Dungeon Meshi',
    });
    expect(back).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('2 votes')).toBeInTheDocument();
    await user.click(back);
    await screen.findByRole('button', { name: 'Vote for Dungeon Meshi' });
    expect(log.calls.map((c) => c.path)).toEqual(['vote:1', 'unvote:1']);
  });

  it('claims a request with a note and gives it up again', async () => {
    const user = userEvent.setup();
    const log = useRequestsApi([request('1', { title: 'Dungeon Meshi' })]);
    renderAt('/library/requests');
    await user.click(await screen.findByRole('button', { name: "I'll upload this" }));
    await user.type(screen.getByLabelText('Note for others (optional)'), 'scanning vol 1');
    await user.click(screen.getByRole('button', { name: 'Tell everyone' }));
    expect(await screen.findByText('You said you will upload this')).toBeInTheDocument();
    expect(screen.getByText(/scanning vol 1/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Start a submission' })).toHaveAttribute(
      'href',
      '/library/submissions',
    );
    await user.click(screen.getByRole('button', { name: 'Not uploading after all' }));
    expect(await screen.findByRole('button', { name: "I'll upload this" })).toBeInTheDocument();
    expect(log.calls).toEqual([
      { path: 'claim:1', body: { note: 'scanning vol 1' } },
      { path: 'release:1', body: undefined },
    ]);
  });

  it('requests a title and refreshes the board', async () => {
    const user = userEvent.setup();
    const log = useRequestsApi([]);
    renderAt('/library/requests');
    await user.click(await screen.findByRole('button', { name: 'Request a title' }));
    await user.type(screen.getByLabelText('Title'), 'Delicious in Dungeon');
    await user.type(screen.getByLabelText('Year'), '2014');
    await user.click(screen.getByRole('button', { name: 'Send request' }));
    await waitFor(() => expect(log.calls[0]?.path).toBe('create'));
    expect(log.calls[0]?.body).toMatchObject({
      section: 'library',
      title: 'Delicious in Dungeon',
      year: 2014,
    });
    expect(await screen.findByText(/Delicious in Dungeon/)).toBeInTheDocument();
  });
});
