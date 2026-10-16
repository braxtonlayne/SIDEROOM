// ADM-COM-001 / H-40: the Server Directory queue in the Review Queues hub.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const listing = (over: Record<string, unknown> = {}) => ({
  guild_id: '900',
  status: 'pending',
  category: 'gaming',
  description: 'Boards and clocks',
  featured: false,
  requested_at: '2026-10-01T10:00:00.000000+00:00',
  decided_at: null,
  decision_note: null,
  guild_name: 'Chess Club',
  requested_by: '5',
  approximate_member_count: 12,
  community_enabled: true,
  ...over,
});

async function openQueue() {
  openAdmin('review');
  const user = userEvent.setup();
  await user.click(await screen.findByRole('tab', { name: /Directory/ }, { timeout: 10_000 }));
  return user;
}

describe('Server Directory queue', () => {
  it('previews a pending request as members will see it, without a join button', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_DIRECTORY'] });
    server.use(
      http.get('*/api/v1/admin/directory', () => HttpResponse.json([listing()])),
      http.get('*/api/v1/admin/directory/queue/900/preview', () =>
        HttpResponse.json({
          id: '900',
          name: 'Chess Club',
          description: 'Boards and clocks',
          icon: null,
          banner: null,
          category: 'gaming',
          featured: false,
          approximate_member_count: 12,
          preferred_locale: 'en-US',
          vanity_url_code: null,
          pending: true,
        }),
      ),
    );
    const user = await openQueue();
    expect(await screen.findByText('Chess Club')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Preview Chess Club' }));
    const dialog = await screen.findByRole('dialog', { name: 'Preview of Chess Club' });
    expect(within(dialog).getByText(/Members do not see this listing yet/)).toBeInTheDocument();
    expect(await within(dialog).findByText('12 members')).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: /join/i })).toBeNull();
  });

  it('approves with a note', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_DIRECTORY'] });
    let queue = [listing()];
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/directory', () => HttpResponse.json(queue)),
      http.patch('*/api/v1/admin/directory/900', async ({ request }) => {
        sent = await readJson(request);
        queue = [];
        return HttpResponse.json({ guild_id: '900', status: 'approved' });
      }),
    );
    const user = await openQueue();
    await user.click(await screen.findByRole('button', { name: 'Approve Chess Club' }));
    const dialog = await screen.findByRole('dialog', { name: 'Approve Chess Club?' });
    await user.type(within(dialog).getByLabelText(/Note to the server managers/), 'Welcome');
    await user.click(within(dialog).getByRole('button', { name: 'Approve' }));
    expect(await screen.findByText('Listing approved.')).toBeInTheDocument();
    expect(sent).toEqual({ decision: 'approve', note: 'Welcome' });
    await waitFor(() => expect(screen.getByText('No requests are waiting.')).toBeInTheDocument());
  });

  it('features a listed server', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_DIRECTORY'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/directory', ({ request }) =>
        HttpResponse.json(
          new URL(request.url).searchParams.get('status') === 'approved'
            ? [listing({ status: 'approved' })]
            : [],
        ),
      ),
      http.patch('*/api/v1/admin/directory/900', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ guild_id: '900', status: 'approved', featured: true });
      }),
    );
    const user = await openQueue();
    await user.click(await screen.findByRole('button', { name: 'Listed' }));
    await user.click(await screen.findByRole('button', { name: 'Feature Chess Club' }));
    await waitFor(() => expect(sent).toEqual({ decision: 'approve', featured: true }));
  });
});
