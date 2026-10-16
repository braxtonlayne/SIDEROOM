import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { setUpdateReady } from '../../pwa/state';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const announcement = (id: string, severity = 'info') => ({
  id,
  title: `Title ${id}`,
  body: `Body ${id}`,
  severity,
  send_dm: false,
  created_by: '9',
  created_at_ms: 1,
  starts_at_ms: 1,
});

describe('LNG-SHL-009 notices bar', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    setUpdateReady(null);
  });
  afterEach(() => setUpdateReady(null));

  it('shows platform announcements and remembers a dismissal', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () =>
        HttpResponse.json([announcement('a1'), announcement('a2', 'critical')]),
      ),
    );
    const first = at('/channels/g1/c1');
    const bar = await screen.findByRole('region', { name: 'Notices' });
    expect(await within(bar).findByText('Title a1')).toBeInTheDocument();
    expect(within(bar).getByText('Title a2')).toBeInTheDocument();
    // Critical announcements cannot be dismissed.
    expect(
      within(bar).queryByRole('button', { name: 'Dismiss announcement: Title a2' }),
    ).toBeNull();
    await userEvent.click(
      within(bar).getByRole('button', { name: 'Dismiss announcement: Title a1' }),
    );
    expect(within(bar).queryByText('Title a1')).toBeNull();
    first.unmount();

    // A reload: a1 stays dismissed, a2 is still there.
    at('/channels/g1/c1');
    const again = await screen.findByRole('region', { name: 'Notices' });
    expect(await within(again).findByText('Title a2')).toBeInTheDocument();
    expect(within(again).queryByText('Title a1')).toBeNull();
  });

  it('offers a reload when an update is waiting', async () => {
    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([])));
    const apply = vi.fn();
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull();
    act(() => setUpdateReady(apply));
    const bar = await screen.findByRole('region', { name: 'Notices' });
    await userEvent.click(within(bar).getByRole('button', { name: 'Reload to update' }));
    expect(apply).toHaveBeenCalledOnce();
    await userEvent.click(within(bar).getByRole('button', { name: 'Dismiss update notice' }));
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull());
  });

  it('stays quiet when the announcements route fails', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () => new HttpResponse(null, { status: 500 })),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull();
  });

  describe('second-factor notice (H-21)', () => {
    const old = '1';
    const setup = (
      opts: {
        mfa?: boolean;
        id?: string;
        passkeys?: unknown[];
        settings?: Record<string, unknown>;
      } = {},
    ) => {
      const patches: unknown[] = [];
      server.use(
        http.get('*/api/v1/announcements/active', () => HttpResponse.json([])),
        http.get('*/api/v1/users/@me', () =>
          HttpResponse.json({
            id: opts.id ?? old,
            username: 'me',
            mfa_enabled: opts.mfa ?? false,
          }),
        ),
        http.get('*/api/v1/sr/auth/passkeys', () => HttpResponse.json(opts.passkeys ?? [])),
        http.get('*/api/v1/users/@me/settings', () =>
          HttpResponse.json({ version: 1, settings: opts.settings ?? {} }),
        ),
        http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
          patches.push(await request.json());
          return HttpResponse.json({ version: 2, settings: { securityNudgeDone: true } });
        }),
      );
      return patches;
    };

    it('shows once for an older account with no TOTP or passkey, and stores the dismissal', async () => {
      const patches = setup();
      at('/channels/g1/c1');
      const bar = await screen.findByRole('region', { name: 'Notices' });
      expect(
        await within(bar).findByText('Protect your account with a second factor.'),
      ).toBeInTheDocument();
      expect(within(bar).getByRole('button', { name: 'Open My Account' })).toBeInTheDocument();
      await userEvent.click(
        within(bar).getByRole('button', { name: 'Dismiss the second-factor notice' }),
      );
      await waitFor(() => expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull());
      expect(patches).toEqual([{ settings: { securityNudgeDone: true } }]);
    });

    it.each([
      ['TOTP is on', { mfa: true }],
      ['a passkey exists', { passkeys: [{ id: 'p1' }] }],
      ['already dismissed', { settings: { securityNudgeDone: true } }],
      [
        'the account is new (welcome dialog window)',
        { id: String(BigInt(Date.now() - 1767225600000 - 1000) << 22n) },
      ],
    ])('stays quiet when %s', async (_name, opts) => {
      setup(opts);
      at('/channels/g1/c1');
      await screen.findByTestId('channel-sidebar');
      await new Promise((r) => setTimeout(r, 150));
      expect(screen.queryByText('Protect your account with a second factor.')).toBeNull();
    });
  });
});
