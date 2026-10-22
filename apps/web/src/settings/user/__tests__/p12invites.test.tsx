import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

const OPEN = {
  code: 'ABCDE-12345',
  link: '/join/ABCDE-12345',
  note: 'for Sam',
  created_at_ms: 1_760_000_000_000,
  redeemed_by: null,
  redeemed_at_ms: null,
};
const REDEEMED = {
  code: 'ZZZZZ-99999',
  link: '/join/ZZZZZ-99999',
  note: '',
  created_at_ms: 1_750_000_000_000,
  redeemed_by: { id: '77', username: 'sam' },
  redeemed_at_ms: 1_751_000_000_000,
};

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Invites (G-57)', () => {
  it('shows credits, open codes and who joined', async () => {
    server.use(
      http.get('*/api/v1/sr/invites', () =>
        HttpResponse.json({ credits: 3, open: [OPEN], redeemed: [REDEEMED], blocked: null }),
      ),
    );
    openSettings('invites');
    expect(await screen.findByText('You can bring 3 more people.')).toBeInTheDocument();
    expect(screen.getByText('ABCDE-12345')).toBeInTheDocument();
    expect(screen.getByText(/for Sam/)).toBeInTheDocument();
    expect(screen.getByText(/@sam joined/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create invite' })).toBeEnabled();
  });

  it('explains why a code cannot be made, and keeps the frozen wording generic', async () => {
    server.use(
      http.get('*/api/v1/sr/invites', () =>
        HttpResponse.json({
          credits: 2,
          open: [],
          redeemed: [],
          blocked: { reason: 'frozen', days: null },
        }),
      ),
    );
    openSettings('invites');
    expect(
      await screen.findByText('Invite creation is paused for review.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create invite' })).toBeDisabled();
  });

  it('creates a code with a note and revokes an open one', async () => {
    const user = userEvent.setup();
    let sent: unknown;
    let revoked = '';
    server.use(
      http.get('*/api/v1/sr/invites', () =>
        HttpResponse.json({ credits: 2, open: [OPEN], redeemed: [], blocked: null }),
      ),
      http.post('*/api/v1/sr/invites', async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json(
          { ...OPEN, code: 'QQQQQ-11111', link: '/join/QQQQQ-11111', note: 'for Kim' },
          { status: 201 },
        );
      }),
      http.delete('*/api/v1/sr/invites/:code', ({ params }) => {
        revoked = String(params.code);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('invites');
    await user.type(await screen.findByLabelText(/Note/), 'for Kim');
    await user.click(screen.getByRole('button', { name: 'Create invite' }));
    await waitFor(() => expect(sent).toEqual({ note: 'for Kim' }));
    expect(await screen.findByText('QQQQQ-11111')).toBeInTheDocument();
    const row = screen.getByText('ABCDE-12345').closest('li') as HTMLElement;
    await user.click(within(row).getByRole('button', { name: 'Revoke invite ABCDE-12345' }));
    await waitFor(() => expect(revoked).toBe('ABCDE-12345'));
  });
});
