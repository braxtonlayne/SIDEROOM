import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const rate = { cents_per_btc: 6_500_000, source: 'median(a,b)', age_secs: 4, stale: false };

const STATUS = {
  enabled: true,
  wallet: { network: 'signet', fingerprint: 'a1b2c3d4', next_index: 7, gap_limit: 20, gap_used: 3 },
  watcher: {
    last_run_ms: Date.now() - 5000,
    last_ok_ms: Date.now() - 5000,
    last_error: null,
    tip_height: 812345,
    polled: 4,
  },
  counts: { confirmed: 12, underpaid: 1 },
  needs_attention: 1,
  rate,
  rate_sources: [
    { name: 'a', last_cents: 6_500_000, last_ok_ms: Date.now(), last_error: null },
    { name: 'b', last_cents: null, last_ok_ms: null, last_error: 'timeout' },
  ],
};

const invoice = (over: Record<string, unknown> = {}) => ({
  id: '501',
  purpose: 'patron',
  state: 'underpaid',
  usd_cents: 500,
  sats: 7693,
  btc: '0.00007693',
  address: 'tb1qexampleaddress',
  bip21: 'bitcoin:tb1qexampleaddress',
  network: 'signet',
  guild_id: null,
  months: 1,
  credits: null,
  anonymous: false,
  rate,
  confirmations_required: 1,
  confirmations: 1,
  received_sats: 3000,
  confirmed_sats: 3000,
  shortfall_sats: 4693,
  excess_sats: 0,
  late: false,
  created_at_ms: 1_700_000_000_000,
  expires_at_ms: 1_700_003_600_000,
  quote_expires_at_ms: 1_700_001_800_000,
  credited_at_ms: null,
  explorer_url: null,
  txs: [{ txid: 'deadbeef0001', amount_sats: 3000, confirmations: 1, dropped: false }],
  resolution: null,
  user_id: '100',
  user_name: 'alice',
  derivation_index: 4,
  resolution_note: null,
  resolved_by: null,
  audit: [{ at_ms: 1_700_000_000_000, kind: 'created', detail: { sats: 7693 } }],
  ...over,
});

const CONFIG = {
  enabled: true,
  confirmations: 1,
  finality_confirmations: 6,
  invoice_expiry_minutes: 60,
  rate_lock_minutes: 30,
  tolerance_bps: 100,
  late_watch_days: 30,
  min_invoice_sats: 1000,
  min_donation_cents: 100,
  max_donation_cents: 1_000_000,
  max_open_invoices: 10,
  poll_secs: 30,
  patron_price_cents: 500,
  patron_max_months: 12,
  patron_days_per_month: 30,
  grace_days: 3,
  level_thresholds_cents: [1000, 2500, 5000],
  pool_window_days: 30,
  invite_credit_price_cents: 2000,
  max_invite_credits: 10,
  hosting_cost_cents: 12800,
  free: {
    upload_mib: 30,
    video_profiles: [],
    video_senders: 5,
    stream_viewers: 25,
    audio_kbps: 96,
    watch_parties: 2,
    downloads_per_day: 0,
    exempt_from_sender_limit: false,
  },
  patron: {
    upload_mib: 500,
    video_profiles: [],
    video_senders: 0,
    stream_viewers: 0,
    audio_kbps: 0,
    watch_parties: 0,
    downloads_per_day: 20,
    exempt_from_sender_limit: true,
  },
  levels: [
    {
      upload_mib: 50,
      video_profiles: [],
      video_senders: 8,
      stream_viewers: 50,
      audio_kbps: 128,
      watch_parties: 4,
      downloads_per_day: 0,
      exempt_from_sender_limit: false,
    },
    {
      upload_mib: 100,
      video_profiles: [],
      video_senders: 12,
      stream_viewers: 50,
      audio_kbps: 256,
      watch_parties: 8,
      downloads_per_day: 0,
      exempt_from_sender_limit: false,
    },
    {
      upload_mib: 250,
      video_profiles: [],
      video_senders: 25,
      stream_viewers: 100,
      audio_kbps: 384,
      watch_parties: 16,
      downloads_per_day: 0,
      exempt_from_sender_limit: false,
    },
  ],
  explorer_tx_url: 'https://mempool.space/tx/{txid}',
};

const grant = (over: Record<string, unknown> = {}) => ({
  id: '800',
  kind: 'patron',
  user_id: '100',
  guild_id: null,
  source: 'admin',
  source_id: '1',
  days: 14,
  ends_at_ms: null,
  permanent: false,
  usd_cents: null,
  starts_at_ms: 1_700_000_000_000,
  revoked_at_ms: null,
  reason: 'giveaway',
  ...over,
});

beforeEach(() => {
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
});

describe('Admin donations access', () => {
  it('lists the Money sections only for MANAGE_DONATIONS', async () => {
    asStaff({ perms: ['MANAGE_DONATIONS'] });
    server.use(http.get('*/api/v1/admin/donations/status', () => HttpResponse.json(STATUS)));
    server.use(
      http.get('*/api/v1/admin/donations/report', () =>
        HttpResponse.json({
          from_ms: 0,
          to_ms: 1,
          credited_cents: 0,
          by_purpose: [],
          by_day: [],
          by_state: {},
        }),
      ),
    );
    openAdmin('donations');
    await screen.findByRole('heading', { name: 'Donations' });
    for (const name of ['Donations', 'Invoices', 'Entitlements', 'Pricing and perks'])
      expect(within(nav()).getByRole('button', { name })).toBeInTheDocument();
  });

  it('hides them from other staff', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json({})));
    openAdmin('dashboard');
    await waitFor(() => expect(nav()).toBeInTheDocument());
    expect(within(nav()).queryByRole('button', { name: 'Invoices' })).toBeNull();
    expect(within(nav()).queryByRole('button', { name: 'Pricing and perks' })).toBeNull();
  });
});

describe('Donations overview', () => {
  beforeEach(() => {
    asStaff({ perms: ['MANAGE_DONATIONS'] });
    server.use(
      http.get('*/api/v1/admin/donations/status', () => HttpResponse.json(STATUS)),
      http.get('*/api/v1/admin/donations/report', () =>
        HttpResponse.json({
          from_ms: 0,
          to_ms: 1,
          credited_cents: 2500,
          by_purpose: [{ purpose: 'patron', cents: 2500, count: 5 }],
          by_day: [],
          by_state: {},
        }),
      ),
    );
  });

  it('shows health, the wallet fingerprint (never the descriptor), and rate sources', async () => {
    openAdmin('donations');
    expect(await screen.findByText('Healthy')).toBeInTheDocument();
    expect(screen.getByText('812345')).toBeInTheDocument();
    expect(screen.getByText('a1b2c3d4')).toBeInTheDocument();
    expect(screen.getByText(/3 of 20/)).toBeInTheDocument();
    expect(screen.getByText('timeout')).toBeInTheDocument();
    expect(screen.getAllByText('$65,000.00', { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getByText('$25.00')).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/[xt]pub[1-9A-HJ-NP-Za-km-z]{20,}/);
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('tests a descriptor, then saves it with a reason', async () => {
    let putBody: Record<string, unknown> = {};
    server.use(
      http.post('*/api/v1/admin/donations/wallet/test', async ({ request }) => {
        const b = (await readJson(request)) as { descriptor: string };
        if (b.descriptor.includes('xprv'))
          return HttpResponse.json(
            { code: 50035, message: 'the descriptor contains private key material' },
            { status: 400 },
          );
        return HttpResponse.json({
          fingerprint: 'ffeeddcc',
          addresses: ['tb1qfirst', 'tb1qsecond'],
        });
      }),
      http.put('*/api/v1/admin/donations/wallet', async ({ request }) => {
        putBody = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json({ network: 'signet', fingerprint: 'ffeeddcc', addresses: [] });
      }),
    );
    openAdmin('donations');
    const box = await screen.findByLabelText('Output descriptor (watch-only)');
    const useBtn = screen.getByRole('button', { name: 'Use this wallet' });
    expect(useBtn).toBeDisabled();

    await userEvent.type(box, 'wpkh(xprvJUNK/0/*)');
    await userEvent.click(screen.getByRole('button', { name: 'Test derivation' }));
    expect(await screen.findByText(/private key material/)).toBeInTheDocument();
    expect(useBtn).toBeDisabled();

    await userEvent.clear(box);
    await userEvent.type(box, 'wpkh(xpubOK/0/*)');
    await userEvent.selectOptions(screen.getByLabelText('Network'), 'signet');
    await userEvent.click(screen.getByRole('button', { name: 'Test derivation' }));
    expect(await screen.findByText('tb1qfirst')).toBeInTheDocument();
    await userEvent.click(useBtn);
    await userEvent.type(await screen.findByLabelText(/Reason/), 'initial setup');
    await userEvent.click(screen.getByRole('button', { name: 'Use wallet' }));
    expect(await screen.findByText('Wallet ffeeddcc is active on signet.')).toBeInTheDocument();
    expect(putBody).toMatchObject({
      descriptor: 'wpkh(xpubOK/0/*)',
      network: 'signet',
      reason: 'initial setup',
    });
    // The descriptor box is cleared after saving: nothing lingers on the page.
    expect(screen.getByLabelText('Output descriptor (watch-only)')).toHaveValue('');
  });

  it('warns when the watcher has a problem and when donations are off', async () => {
    server.use(
      http.get('*/api/v1/admin/donations/status', () =>
        HttpResponse.json({
          ...STATUS,
          enabled: false,
          wallet: null,
          rate: null,
          watcher: {
            ...STATUS.watcher,
            last_ok_ms: null,
            last_error: '2 of 4 invoices could not be read',
          },
        }),
      ),
    );
    openAdmin('donations');
    expect(await screen.findByText('The watcher reported a problem')).toBeInTheDocument();
    expect(screen.getByText('Donations are off')).toBeInTheDocument();
    expect(screen.getByText('No price')).toBeInTheDocument();
  });
});

describe('Donation invoices', () => {
  beforeEach(() => asStaff({ perms: ['MANAGE_DONATIONS'] }));

  it('lists, searches and opens an invoice, then resolves it with a reason', async () => {
    const queries: string[] = [];
    let resolveBody: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/admin/donations/invoices', ({ request }) => {
        queries.push(new URL(request.url).search);
        return HttpResponse.json({ items: [invoice()], next_before: null });
      }),
      http.get('*/api/v1/admin/donations/invoices/501', () => HttpResponse.json(invoice())),
      http.post('*/api/v1/admin/donations/invoices/501/resolve', async ({ request }) => {
        resolveBody = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(
          invoice({
            state: 'resolved',
            resolution: 'mark_paid',
            resolution_note: 'rest paid off-chain',
          }),
        );
      }),
    );
    openAdmin('donation-invoices');
    const table = await screen.findByRole('table', { name: 'Invoices' });
    expect(within(table).getByText('alice')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);

    await userEvent.type(screen.getByLabelText('Search'), 'deadbeef');
    await userEvent.click(screen.getByRole('button', { name: 'Search' }));
    await waitFor(() => expect(queries.some((q) => q.includes('q=deadbeef'))).toBe(true));

    const fresh = await screen.findByRole('table', { name: 'Invoices' });
    await userEvent.click(within(fresh).getByRole('button', { name: '501' }));
    const dialog = await screen.findByRole('dialog', { name: 'Invoice 501' });
    expect(within(dialog).getByText('deadbeef0001')).toBeInTheDocument();
    expect(within(dialog).getByText(/short by 4,693 sats/)).toBeInTheDocument();
    expect(within(dialog).getByText(/created/)).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole('button', { name: 'Resolve by hand' }));
    const reasonDialog = await screen.findByRole('dialog', { name: 'Resolve this invoice' });
    const confirm = within(reasonDialog).getByRole('button', { name: 'Resolve' });
    expect(confirm).toBeDisabled();
    await userEvent.type(within(reasonDialog).getByLabelText(/Note/), 'rest paid off-chain');
    await userEvent.type(within(reasonDialog).getByLabelText(/Reason/), 'ticket 12');
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(resolveBody).toEqual({
        resolution: 'mark_paid',
        note: 'rest paid off-chain',
        reason: 'ticket 12',
      }),
    );
    expect(await within(dialog).findByText(/mark paid: rest paid off-chain/)).toBeInTheDocument();
  });

  it('shows an empty state', async () => {
    server.use(
      http.get('*/api/v1/admin/donations/invoices', () =>
        HttpResponse.json({ items: [], next_before: null }),
      ),
    );
    openAdmin('donation-invoices');
    expect(await screen.findByText('No invoices')).toBeInTheDocument();
  });
});

describe('Donation entitlements', () => {
  beforeEach(() => asStaff({ perms: ['MANAGE_DONATIONS'] }));

  it('grants Patron time and revokes a grant, each with a reason', async () => {
    let list = [grant()];
    let granted: Record<string, unknown> = {};
    let revoked: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/admin/donations/entitlements', () =>
        HttpResponse.json({ items: list, next_before: null }),
      ),
      http.post('*/api/v1/admin/donations/entitlements', async ({ request }) => {
        granted = (await readJson(request)) as Record<string, unknown>;
        list = [grant({ id: '801', user_id: '200' }), ...list];
        return HttpResponse.json(list[0]);
      }),
      http.delete('*/api/v1/admin/donations/entitlements/800', async ({ request }) => {
        revoked = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(grant({ revoked_at_ms: 1 }));
      }),
    );
    openAdmin('donation-entitlements');
    expect(await screen.findByText('Patron, 14 days')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);

    await userEvent.click(screen.getByRole('button', { name: 'Grant' }));
    const dlg = await screen.findByRole('dialog', { name: 'Grant an entitlement' });
    const ok = within(dlg).getByRole('button', { name: 'Grant' });
    expect(ok).toBeDisabled();
    await userEvent.type(within(dlg).getByLabelText('Account id'), '200');
    await userEvent.clear(within(dlg).getByLabelText('Days'));
    await userEvent.type(within(dlg).getByLabelText('Days'), '30');
    await userEvent.type(within(dlg).getByLabelText(/Reason/), 'contest winner');
    await userEvent.click(ok);
    await waitFor(() =>
      expect(granted).toEqual({
        kind: 'patron',
        user_id: '200',
        days: 30,
        reason: 'contest winner',
      }),
    );
    expect(await screen.findByText('Entitlement granted.')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /Revoke Patron, 14 days for 100/ }));
    const rdlg = await screen.findByRole('dialog', { name: 'Revoke this entitlement' });
    await userEvent.type(within(rdlg).getByLabelText(/Reason/), 'mistake');
    await userEvent.click(within(rdlg).getByRole('button', { name: 'Revoke' }));
    await waitFor(() => expect(revoked).toEqual({ reason: 'mistake' }));
  });
});

describe('Pricing and perks', () => {
  beforeEach(() => {
    asStaff({ perms: ['MANAGE_DONATIONS'] });
    server.use(http.get('*/api/v1/admin/donations/config', () => HttpResponse.json(CONFIG)));
  });

  it('saves only the changed settings, with a reason', async () => {
    let patch: Record<string, unknown> = {};
    server.use(
      http.patch('*/api/v1/admin/donations/config', async ({ request }) => {
        patch = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json({ ...CONFIG, patron_price_cents: 700 });
      }),
    );
    openAdmin('donation-pricing');
    const price = await screen.findByLabelText('Patron price per month (USD cents)');
    const save = screen.getByRole('button', { name: 'Save changes' });
    expect(save).toBeDisabled();
    await userEvent.clear(price);
    await userEvent.type(price, '700');
    const upload = screen.getByLabelText('Level 2 Upload MB');
    await userEvent.clear(upload);
    await userEvent.type(upload, '150');
    await userEvent.click(save);
    const dlg = await screen.findByRole('dialog', { name: 'Save donation settings' });
    await userEvent.type(within(dlg).getByLabelText(/Reason/), 'price change');
    await userEvent.click(within(dlg).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(patch.reason).toBe('price change'));
    const changes = patch.changes as Record<string, unknown>;
    expect(Object.keys(changes).sort()).toEqual(['levels', 'patron_price_cents']);
    expect(changes.patron_price_cents).toBe(700);
    expect((changes.levels as { upload_mib: number }[])[1]!.upload_mib).toBe(150);
    expect(await screen.findByText('Saved.')).toBeInTheDocument();
  });

  it('runs the cost calculator against the configured prices', async () => {
    openAdmin('donation-pricing');
    await screen.findByRole('heading', { name: 'Cost calculator' });
    // $128 server, $5 Patron, $10/$25/$50 levels: 26 Patrons, 13/6/3 servers (docs/08 section 6.3).
    expect(screen.getByTestId('break-even')).toHaveTextContent(
      'you need 26 Patrons, or 13 level-1 servers, or 6 level-2 servers, or 3 level-3 servers',
    );
    const cost = screen.getByLabelText('Server cost per month (USD)');
    await userEvent.clear(cost);
    await userEvent.type(cost, '50');
    expect(screen.getByTestId('break-even')).toHaveTextContent('you need 10 Patrons');
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('hides a donor from the boards with a reason', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post('*/api/v1/admin/donations/leaderboard/hide', async ({ request }) => {
        body = (await readJson(request)) as Record<string, unknown>;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('donation-pricing');
    await userEvent.type(await screen.findByLabelText('Id'), '4242');
    await userEvent.click(screen.getByRole('button', { name: 'Hide' }));
    const dlg = await screen.findByRole('dialog', { name: 'Hide from the leaderboards' });
    await userEvent.type(within(dlg).getByLabelText(/Reason/), 'offensive name');
    await userEvent.click(within(dlg).getByRole('button', { name: 'Hide' }));
    await waitFor(() =>
      expect(body).toEqual({ user_id: '4242', hidden: true, reason: 'offensive name' }),
    );
    expect(await screen.findByText('Hidden from the boards.')).toBeInTheDocument();
  });
});
