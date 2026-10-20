// N-05 (D-324): the Settings, Privacy card "Delete all my messages", and the deletion screen's link to it.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(resetPrefs);

const PATH = '*/api/v1/sr/users/@me/messages/delete-all';

describe('Delete all my messages', () => {
  it('shows the count, then runs after the typed username and password', async () => {
    signedIn();
    const seen: string[] = [];
    let posted: unknown;
    server.use(
      http.get(PATH, ({ request }) => {
        seen.push(new URL(request.url).search);
        return HttpResponse.json({ count: 12, reactions: 5, poll_votes: 2, last: null });
      }),
      http.post(PATH, async ({ request }) => {
        posted = await request.json();
        return HttpResponse.json(
          {
            id: '5',
            state: 'queued',
            servers: true,
            dms: false,
            requested_at: '2026-10-20T10:00:00Z',
          },
          { status: 202 },
        );
      }),
    );
    openSettings('privacy');
    expect(
      await screen.findByText('12 messages, 5 reactions, 2 poll votes would be removed.'),
    ).toBeInTheDocument();
    // Un-ticking direct messages asks for the narrower count.
    await userEvent.click(screen.getByLabelText('Messages in direct messages and group DMs'));
    await waitFor(() => expect(seen.some((s) => s.includes('dms=false'))).toBe(true));
    await userEvent.click(screen.getByRole('button', { name: 'Delete my messages' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delete all my messages?' });
    const confirm = within(dialog).getByRole('button', { name: 'Delete messages' });
    expect(confirm).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText(/Type your username, alice/), 'alice');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(posted).toEqual({
        confirm_username: 'alice',
        password: 'hunter22',
        servers: true,
        dms: false,
      }),
    );
    expect(await screen.findByText(/Started\. It runs in the background/)).toBeInTheDocument();
  });

  it('says so while a run is open and keeps the button off', async () => {
    signedIn();
    server.use(
      http.get(PATH, () =>
        HttpResponse.json({
          count: 3,
          last: { id: '5', state: 'running', requested_at: '2026-10-20T10:00:00Z' },
        }),
      ),
    );
    openSettings('privacy');
    expect(await screen.findByText('A bulk delete is running.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete my messages' })).toBeDisabled();
  });

  it('is linked from the deletion screen, which names the invoice years', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, invoice_retention_years: 7 }),
      ),
    );
    openSettings('account');
    await userEvent.click(await screen.findByRole('button', { name: 'Delete my account' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delete your account' });
    expect(
      await within(dialog).findByText(/kept 7 years after the year of payment/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/Messages you leave in place stay/)).toBeInTheDocument();
    expect(within(dialog).getByRole('link', { name: 'Delete all my messages' })).toHaveAttribute(
      'href',
      expect.stringContaining('/settings/user/privacy'),
    );
  });
});
