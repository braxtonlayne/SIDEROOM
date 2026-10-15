import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AppTester } from '../types';
import { APP, http, HttpResponse, openAt, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

describe('application: testers (D-204)', () => {
  it('lists testers, invites by username and removes one', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let testers: AppTester[] = [
      {
        id: '900',
        user_id: '7',
        username: 'bob',
        global_name: 'Bob',
        state: 'accepted',
        created_at: '2026-10-02T00:00:00Z',
      },
    ];
    const invited: unknown[] = [];
    const removed: string[] = [];
    server.use(
      http.get('*/api/v1/applications/500/testers', () => HttpResponse.json(testers)),
      http.post('*/api/v1/applications/500/testers', async ({ request }) => {
        const body = (await request.json()) as { username: string };
        invited.push(body);
        const t: AppTester = {
          id: '901',
          user_id: '8',
          username: body.username,
          global_name: null,
          state: 'invited',
          created_at: '2026-10-03T00:00:00Z',
        };
        testers = [...testers, t];
        return HttpResponse.json(t, { status: 201 });
      }),
      http.delete('*/api/v1/applications/500/testers/:uid', ({ params }) => {
        removed.push(String(params.uid));
        testers = testers.filter((t) => t.user_id !== params.uid);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAt('/developers/applications/500?tab=installation');
    expect(await screen.findByText('Bob')).toBeInTheDocument();
    expect(screen.getByText('Accepted')).toBeInTheDocument();

    await user.type(screen.getByLabelText('Username'), '@carol');
    await user.click(screen.getByRole('button', { name: 'Invite tester' }));
    await waitFor(() => expect(invited).toEqual([{ username: 'carol' }]));
    expect(await screen.findByText('Invited')).toBeInTheDocument();
    expect(screen.getByLabelText('Username')).toHaveValue('');

    await user.click(screen.getByRole('button', { name: 'Remove Bob as a tester' }));
    await waitFor(() => expect(removed).toEqual(['7']));
    await waitFor(() => expect(screen.queryByText('Bob')).not.toBeInTheDocument());
  });

  it('shows the server refusal when the invitation fails', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/installs', () => HttpResponse.json([])),
      http.get('*/api/v1/applications/500/testers', () => HttpResponse.json([])),
      http.post('*/api/v1/applications/500/testers', () =>
        HttpResponse.json({ code: 10013, message: 'Unknown User' }, { status: 404 }),
      ),
    );
    openAt('/developers/applications/500?tab=installation');
    await screen.findByText('No testers yet.');
    await user.type(screen.getByLabelText('Username'), 'nobody');
    await user.click(screen.getByRole('button', { name: 'Invite tester' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Unknown User');
  });

  it('disables inviting at 25 testers', async () => {
    serveApp({ ...APP });
    const many = Array.from({ length: 25 }, (_, i) => ({
      id: String(1000 + i),
      user_id: String(2000 + i),
      username: `tester${i}`,
      global_name: null,
      state: 'accepted',
      created_at: '2026-10-02T00:00:00Z',
    }));
    server.use(http.get('*/api/v1/applications/500/testers', () => HttpResponse.json(many)));
    openAt('/developers/applications/500?tab=installation');
    expect(await screen.findByText(/already has 25 testers/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Invite tester' })).toBeDisabled();
  });

  it('is hidden once the app is released', async () => {
    serveApp({ ...APP, release_state: 'released' });
    openAt('/developers/applications/500?tab=installation');
    await screen.findByText('Installed in');
    expect(screen.queryByText('Testers')).not.toBeInTheDocument();
  });
});
