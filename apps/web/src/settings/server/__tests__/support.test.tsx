import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const perks = {
  upload_mib: 50,
  video_profiles: [{ height: 1080, fps: 30 }],
  video_senders: 8,
  stream_viewers: 50,
  audio_kbps: 128,
  watch_parties: 4,
  downloads_per_day: 0,
  exempt_from_sender_limit: false,
};

const support = (over: object = {}) => ({
  guild_id: 'g1',
  level: 1,
  pool_cents: 1800,
  pool_window_days: 30,
  next_threshold_cents: 2500,
  grace_until_ms: null,
  perks,
  top_contributors: [
    { user_id: '1', name: 'Alice', avatar: null, cents: 1200, anonymous: false },
    { user_id: null, name: 'Anonymous', avatar: null, cents: null, anonymous: true },
  ],
  hidden_from_leaderboard: false,
  is_owner: true,
  ...over,
});

const config = {
  enabled: true,
  network: 'signet',
  confirmations: 1,
  invoice_expiry_minutes: 60,
  rate_lock_minutes: 30,
  patron: { price_cents: 500, max_months: 12, days_per_month: 30, grace_days: 3, perks },
  levels: [],
  free_perks: perks,
  invite_credit_price_cents: 2000,
  max_invite_credits: 10,
  min_donation_cents: 100,
  max_donation_cents: 1_000_000,
  rate: null,
  hosting: { cost_cents: 0, donated_this_month_cents: 0, month_start_ms: 0 },
};

function api(s: object = support(), cfg: object = config) {
  server.use(
    http.get('*/api/v1/guilds/g1/support', () => HttpResponse.json(s)),
    http.get('*/api/v1/donations/config', () => HttpResponse.json(cfg)),
    http.get('*/api/v1/donations/invoices', () =>
      HttpResponse.json({ items: [], next_before: null }),
    ),
  );
}

describe('Server Support settings', () => {
  it('shows the level, progress to the next one, perks and contributors', async () => {
    seed({ perms: [MANAGE.guild] });
    api();
    await openSettings('support');
    expect(await screen.findByRole('heading', { name: /Level 1/ })).toBeInTheDocument();
    const bar = screen.getByRole('progressbar', { name: 'Pool toward the next level' });
    expect(bar).toHaveAttribute('aria-valuenow', '1800');
    expect(bar).toHaveAttribute('aria-valuemax', '2500');
    expect(screen.getByText(/\$18\.00 of \$25\.00 for level 2/)).toBeInTheDocument();
    expect(screen.getByText('8 video senders per voice channel')).toBeInTheDocument();
    const list = screen.getByRole('list', { name: 'Top contributors' });
    expect(within(list).getByText('Alice')).toBeInTheDocument();
    expect(within(list).getByText('Anonymous')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('compares the levels and lists your own contributions to this server', async () => {
    seed({ perms: [MANAGE.guild] });
    const l1 = { ...perks, upload_mib: 100 };
    api(support(), {
      ...config,
      free_perks: { ...perks, upload_mib: 25 },
      levels: [{ level: 1, threshold_cents: 2500, perks: l1 }],
    });
    const inv = (id: string, guild: string, purpose: string, cents: number) => ({
      id,
      purpose,
      state: 'confirmed',
      usd_cents: cents,
      guild_id: guild,
      anonymous: false,
      created_at_ms: Date.parse('2030-01-05T00:00:00Z'),
    });
    server.use(
      http.get('*/api/v1/donations/invoices', () =>
        HttpResponse.json({
          items: [
            inv('1', 'g1', 'server_level', 1200),
            inv('2', 'g2', 'server_level', 3300),
            inv('3', 'g1', 'donate', 4400),
          ],
          next_before: null,
        }),
      ),
    );
    await openSettings('support');
    const table = await screen.findByRole('table', { name: /What each server level gives/ });
    expect(within(table).getByRole('columnheader', { name: /Level 0/ })).toBeInTheDocument();
    expect(within(table).getByRole('columnheader', { name: /Level 1/ })).toHaveAttribute(
      'aria-current',
      'true',
    );
    expect(within(table).getByText('25 MiB')).toBeInTheDocument();
    expect(within(table).getByText('100 MiB')).toBeInTheDocument();
    const mine = screen.getByRole('list', { name: 'Your contributions' });
    expect(within(mine).getAllByRole('listitem')).toHaveLength(1);
    expect(within(mine).getByText('$12.00')).toBeInTheDocument();
  });

  it('warns while the level is held in the grace period', async () => {
    seed({ perms: [MANAGE.guild] });
    api(support({ grace_until_ms: Date.parse('2030-01-05T00:00:00Z'), pool_cents: 400 }));
    await openSettings('support');
    expect(await screen.findByText('The pool is below this level')).toBeInTheDocument();
  });

  it('creates an anonymous contribution invoice for this server', async () => {
    seed({ perms: [MANAGE.guild] });
    api();
    let body: unknown;
    server.use(
      http.post('*/api/v1/donations/invoices', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(
          {
            id: '9',
            purpose: 'server_level',
            state: 'open',
            usd_cents: 1000,
            sats: 15385,
            btc: '0.00015385',
            address: 'tb1qexample',
            bip21: 'bitcoin:tb1qexample?amount=0.00015385',
            network: 'signet',
            guild_id: 'g1',
            months: null,
            credits: null,
            anonymous: true,
            rate: { cents_per_btc: 6_500_000, source: 'fake', age_secs: 0, stale: false },
            confirmations_required: 1,
            confirmations: null,
            received_sats: 0,
            confirmed_sats: 0,
            shortfall_sats: 15385,
            excess_sats: 0,
            late: false,
            created_at_ms: Date.now(),
            expires_at_ms: Date.now() + 3_600_000,
            quote_expires_at_ms: Date.now() + 1_800_000,
            credited_at_ms: null,
            explorer_url: null,
            txs: [],
            resolution: null,
          },
          { status: 201 },
        );
      }),
    );
    await openSettings('support');
    await userEvent.click(await screen.findByRole('switch', { name: /Contribute anonymously/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Support $10.00' }));
    expect(await screen.findByRole('dialog', { name: 'Pay with Bitcoin' })).toBeInTheDocument();
    expect(body).toEqual({
      purpose: 'server_level',
      guild_id: 'g1',
      usd_cents: 1000,
      anonymous: true,
    });
  });

  it('lets only the owner hide the server from the leaderboard', async () => {
    seed({ perms: [MANAGE.guild], ownerId: '1' });
    api();
    let patch: unknown;
    server.use(
      http.patch('*/api/v1/guilds/g1/support', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({ hidden_from_leaderboard: true });
      }),
    );
    await openSettings('support');
    await userEvent.click(
      await screen.findByRole('switch', { name: /Hide this server from the public leaderboard/ }),
    );
    await waitFor(() => expect(patch).toEqual({ hidden_from_leaderboard: true }));
  });

  it('hides the owner switch from a manager who is not the owner', async () => {
    seed({ perms: [MANAGE.guild] });
    api(support({ is_owner: false }));
    await openSettings('support');
    await screen.findByRole('heading', { name: /Level 1/ });
    expect(screen.queryByRole('switch', { name: /Hide this server/ })).toBeNull();
  });
});

describe('Leaderboards page', () => {
  const board = (name: string, over: object = {}) => ({
    board: 'donors',
    window: '30d',
    me: { rank: 2, cents: 1200 },
    generated_at_ms: 1,
    items: [
      { rank: 1, user_id: null, guild_id: null, name, avatar: null, cents: 2000, anonymous: true },
      {
        rank: 2,
        user_id: '1',
        guild_id: null,
        name: 'Alice',
        avatar: null,
        cents: null,
        anonymous: false,
      },
    ],
    ...over,
  });

  function open() {
    const router = createAppRouter(
      createMemoryHistory({ initialEntries: ['/support/leaderboards'] }),
    );
    render(<RouterProvider router={router} />);
  }

  it('shows anonymous donors and hidden amounts as the server sent them', async () => {
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/donations/leaderboards/donors', ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('window') ?? '');
        return HttpResponse.json(board('Anonymous patron'));
      }),
    );
    open();
    const list = await screen.findByRole('list', { name: 'Top donors' });
    expect(within(list).getByText('Anonymous patron')).toBeInTheDocument();
    expect(within(list).getByText('$20.00')).toBeInTheDocument();
    expect(within(list).getByText('Amount hidden')).toBeInTheDocument();
    expect(screen.getByText('Your rank: #2 ($12.00)')).toBeInTheDocument();
    expect(seen).toEqual(['30d']);
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('shows each donor picture, and the initials where there is none', async () => {
    server.use(
      http.get('*/api/v1/donations/leaderboards/donors', () =>
        HttpResponse.json(
          board('Anonymous patron', {
            items: [
              {
                rank: 1,
                user_id: '7',
                guild_id: null,
                name: 'Ann Lee',
                avatar: 'abc',
                cents: 900,
                anonymous: false,
              },
              {
                rank: 2,
                user_id: null,
                guild_id: null,
                name: 'Anonymous patron',
                avatar: null,
                cents: null,
                anonymous: true,
              },
            ],
          }),
        ),
      ),
    );
    open();
    const list = await screen.findByRole('list', { name: 'Top donors' });
    const img = list.querySelector('img');
    expect(img?.getAttribute('src')).toBe('/cdn/avatars/7/abc.png?size=64');
    // The initials are generated content (data-initials), not text of the avatar.
    expect(list.querySelector('[data-initials="AP"]')).not.toBeNull();
  });

  it('switches to the lifetime window and the servers tab', async () => {
    const windows: string[] = [];
    server.use(
      http.get('*/api/v1/donations/leaderboards/donors', ({ request }) => {
        windows.push(new URL(request.url).searchParams.get('window') ?? '');
        return HttpResponse.json(board('Someone'));
      }),
      http.get('*/api/v1/donations/leaderboards/servers', () =>
        HttpResponse.json({
          board: 'servers',
          window: '30d',
          me: null,
          generated_at_ms: 1,
          items: [
            {
              rank: 1,
              user_id: null,
              guild_id: '5',
              name: 'Book Club',
              avatar: null,
              cents: 1800,
              anonymous: false,
            },
          ],
        }),
      ),
    );
    open();
    await screen.findByRole('list', { name: 'Top donors' });
    await userEvent.click(screen.getByRole('button', { name: 'Lifetime' }));
    await waitFor(() => expect(windows).toContain('lifetime'));
    await userEvent.click(screen.getByRole('tab', { name: 'Top servers' }));
    const servers = await screen.findByRole('list', { name: 'Top servers' });
    expect(within(servers).getByText('Book Club')).toBeInTheDocument();
  });

  it('shows the contributor boards with counts and your rank, without the time window', async () => {
    const asked: string[] = [];
    server.use(
      http.get('*/api/v1/donations/leaderboards/donors', () => HttpResponse.json(board('Someone'))),
      http.get('*/api/v1/donations/leaderboards/contributors/:kind', ({ params }) => {
        asked.push(String(params.kind));
        return HttpResponse.json({
          kind: params.kind,
          generated_at_ms: 1,
          me: { rank: 2, count: 1 },
          items: [
            { rank: 1, user_id: '9', name: 'Bob', avatar: null, count: 5 },
            { rank: 2, user_id: '1', name: 'Alice', avatar: null, count: 1 },
          ],
        });
      }),
    );
    open();
    await screen.findByRole('list', { name: 'Top donors' });
    await userEvent.click(screen.getByRole('tab', { name: 'Top movie contributors' }));
    const list = await screen.findByRole('list', { name: 'Top movie contributors' });
    expect(within(list).getByText('Bob')).toBeInTheDocument();
    expect(within(list).getByText('5 movies')).toBeInTheDocument();
    expect(within(list).getByText('1 movie')).toBeInTheDocument();
    expect(screen.getByText('Your rank: #2 (1 item)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Lifetime' })).toBeNull();
    await userEvent.click(screen.getByRole('tab', { name: 'Top chapter contributors' }));
    await screen.findByRole('list', { name: 'Top chapter contributors' });
    expect(asked).toEqual(['movies', 'chapters']);
  });

  it('says so when nobody is on the board yet', async () => {
    server.use(
      http.get('*/api/v1/donations/leaderboards/donors', () =>
        HttpResponse.json(board('x', { items: [], me: null })),
      ),
    );
    open();
    expect(await screen.findByText('Nobody on this board yet')).toBeInTheDocument();
  });
});
