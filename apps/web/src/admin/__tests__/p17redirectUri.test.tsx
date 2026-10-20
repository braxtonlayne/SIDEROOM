// M-05 (D-292): Admin > System prints the account-linking redirect URI with a copy button.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const URI = 'https://sr.example.test/api/v1/connections/discord/callback';

describe('Admin > System: account linking redirect URI', () => {
  it('shows the URI, the hint and copies it', async () => {
    asStaff({ perms: [], owner: true });
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    server.use(
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({
          counts: {
            accounts_active: 1,
            accounts_banned: 0,
            accounts_deactivated: 0,
            accounts_deleted: 0,
            jobs_dead: 0,
            jobs_queued: 0,
            jobs_running: 0,
            sessions_active: 1,
            staff: 1,
          },
          gateway_sessions: 1,
          probes: [],
          version: '1',
          integrations: [
            { id: 'discord_oauth', configured: true, detail: null },
            { id: 'discord_connections_redirect', configured: true, detail: URI },
          ],
        }),
      ),
    );
    openAdmin('system');
    const list = await screen.findByRole('list', { name: 'Integrations' });
    expect(within(list).getByText('Account linking redirect URI')).toBeInTheDocument();
    expect(within(list).getByText(URI)).toBeInTheDocument();
    expect(
      within(list).getByText('Register this URI on the same Discord application.'),
    ).toBeInTheDocument();
    await userEvent.click(within(list).getByRole('button', { name: 'Copy' }));
    expect(writeText).toHaveBeenCalledWith(URI);
  });
});
