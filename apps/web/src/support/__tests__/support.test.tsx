import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { InvoiceModal } from '../InvoiceModal';
import { parseDollars, perkLines } from '../SupportPanel';
import type { DonationConfig, Invoice, Me, PerkSet } from '../api';

const perks = (over: Partial<PerkSet> = {}): PerkSet => ({
  upload_mib: 30,
  video_profiles: [{ height: 720, fps: 30 }],
  video_senders: 5,
  stream_viewers: 25,
  audio_kbps: 96,
  watch_parties: 2,
  downloads_per_day: 0,
  exempt_from_sender_limit: false,
  ...over,
});

const CONFIG: DonationConfig = {
  enabled: true,
  network: 'signet',
  confirmations: 1,
  invoice_expiry_minutes: 60,
  rate_lock_minutes: 30,
  patron: {
    price_cents: 500,
    max_months: 12,
    days_per_month: 30,
    grace_days: 3,
    perks: perks({
      upload_mib: 500,
      video_profiles: [{ height: 1080, fps: 60 }],
      video_senders: 0,
      stream_viewers: 0,
      audio_kbps: 0,
      watch_parties: 0,
      downloads_per_day: 20,
      exempt_from_sender_limit: true,
    }),
  },
  levels: [
    { level: 1, threshold_cents: 1000, perks: perks({ upload_mib: 50, video_senders: 8 }) },
    { level: 2, threshold_cents: 2500, perks: perks({ upload_mib: 100, video_senders: 12 }) },
    { level: 3, threshold_cents: 5000, perks: perks({ upload_mib: 250, video_senders: 25 }) },
  ],
  free_perks: perks(),
  invite_credit_price_cents: 2000,
  max_invite_credits: 10,
  min_donation_cents: 100,
  max_donation_cents: 1_000_000,
  rate: { cents_per_btc: 6_500_000, source: 'fake', age_secs: 5, stale: false },
  hosting: { cost_cents: 12800, donated_this_month_cents: 4200, month_start_ms: 0 },
};

const ME: Me = {
  patron: { active: false, in_grace: false, permanent: false, until_ms: null, started_ms: null },
  limits: {
    upload_bytes: 31457280,
    large_upload_expiry_days: null,
    standard_upload_bytes: 31457280,
    video_profiles: [{ height: 720, fps: 30 }],
    video_senders: 5,
    stream_viewers: 25,
    audio_kbps: 96,
    watch_parties: 2,
    downloads_per_day: 0,
    exempt_from_sender_limit: false,
    patron: false,
    guild_level: 0,
  },
  preferences: {
    hide_name: false,
    hide_amount: false,
    hide_badge: false,
    hide_contributions: false,
  },
};

const invoice = (over: Partial<Invoice> = {}): Invoice => ({
  id: '9001',
  purpose: 'patron',
  state: 'open',
  usd_cents: 500,
  sats: 7693,
  btc: '0.00007693',
  address: 'tb1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu',
  bip21: 'bitcoin:tb1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu?amount=0.00007693&label=SIDE%20ROOM',
  network: 'signet',
  guild_id: null,
  months: 1,
  credits: null,
  anonymous: false,
  rate: { cents_per_btc: 6_500_000, source: 'fake', age_secs: 0, stale: false },
  confirmations_required: 1,
  confirmations: null,
  received_sats: 0,
  confirmed_sats: 0,
  shortfall_sats: 7693,
  excess_sats: 0,
  late: false,
  created_at_ms: Date.now(),
  expires_at_ms: Date.now() + 60 * 60_000,
  quote_expires_at_ms: Date.now() + 30 * 60_000,
  credited_at_ms: null,
  explorer_url: null,
  txs: [],
  resolution: null,
  ...over,
});

function signedIn() {
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    http.get('*/api/v1/users/@me', () =>
      HttpResponse.json({ id: '1', username: 'alice', global_name: 'Alice' }),
    ),
  );
}

function supportApi(over: { me?: Me; invoices?: Invoice[]; config?: DonationConfig } = {}) {
  server.use(
    http.get('*/api/v1/donations/config', () => HttpResponse.json(over.config ?? CONFIG)),
    http.get('*/api/v1/donations/me', () => HttpResponse.json(over.me ?? ME)),
    http.get('*/api/v1/donations/invoices', () =>
      HttpResponse.json({ items: over.invoices ?? [], next_before: null }),
    ),
  );
}

function openSupport() {
  const router = createAppRouter(
    createMemoryHistory({ initialEntries: ['/settings/user/support'] }),
  );
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  resetStore();
  signedIn();
});

describe('Support / Patron settings', () => {
  it('shows the hosting bar, the Patron card with prices, and the perks', async () => {
    supportApi();
    openSupport();
    expect(await screen.findByRole('heading', { name: 'Support / Patron' })).toBeInTheDocument();
    const bar = await screen.findByRole('progressbar', { name: /Donations this month/ });
    expect(bar).toHaveAttribute('aria-valuenow', '4200');
    expect(screen.getByText('$42.00 of $128.00 this month')).toBeInTheDocument();
    expect(screen.getByText('You are not a Patron.')).toBeInTheDocument();
    expect(screen.getByText('Uploads up to 500 MB per file')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '12 months, $60.00' })).toBeInTheDocument();
    // The levels explainer carries the thresholds.
    const levels = screen.getByRole('table', { name: 'Server levels' });
    expect(within(levels).getByText('$25.00')).toBeInTheDocument();
  });

  it('is accessible', async () => {
    supportApi();
    openSupport();
    await screen.findByRole('button', { name: '1 month, $5.00' });
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says so when donations are off and keeps the buttons disabled', async () => {
    supportApi({ config: { ...CONFIG, enabled: false, network: null } });
    openSupport();
    expect(await screen.findByText('Donations are not switched on')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '1 month, $5.00' })).toBeDisabled();
  });

  it('creates a Patron invoice and shows the address, amounts and a QR code', async () => {
    supportApi();
    let body: unknown;
    server.use(
      http.post('*/api/v1/donations/invoices', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(invoice({ months: 3, usd_cents: 1500 }), { status: 201 });
      }),
    );
    openSupport();
    await userEvent.click(await screen.findByRole('button', { name: '3 months, $15.00' }));
    const dialog = await screen.findByRole('dialog', { name: 'Pay with Bitcoin' });
    expect(body).toEqual({ purpose: 'patron', months: 3 });
    expect(within(dialog).getByTestId('invoice-address')).toHaveTextContent(
      'tb1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu',
    );
    expect(within(dialog).getByText('7,693 sats (0.00007693 BTC)')).toBeInTheDocument();
    expect(
      within(dialog).getByRole('img', { name: /QR code to pay 7,693 sats/ }),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole('link', { name: 'Open in wallet' })).toHaveAttribute(
      'href',
      expect.stringMatching(/^bitcoin:tb1q/),
    );
    expect(within(dialog).getByRole('timer')).toHaveTextContent(/Expires in/);
    expect(within(dialog).getByRole('list', { name: 'Payment progress' })).toBeInTheDocument();
    expect(await axeViolations(dialog)).toEqual([]);
  });

  it('shows a friendly error when the server refuses (no price, gap limit)', async () => {
    supportApi();
    server.use(
      http.post('*/api/v1/donations/invoices', () =>
        HttpResponse.json(
          { code: 130000, message: 'no current exchange rate is available' },
          { status: 503 },
        ),
      ),
    );
    openSupport();
    await userEvent.click(await screen.findByRole('button', { name: '1 month, $5.00' }));
    expect(await screen.findByText('no current exchange rate is available')).toBeInTheDocument();
  });

  it('validates a custom donation amount before sending', async () => {
    supportApi();
    const calls: unknown[] = [];
    server.use(
      http.post('*/api/v1/donations/invoices', async ({ request }) => {
        calls.push(await request.json());
        return HttpResponse.json(invoice({ purpose: 'donate', months: null, usd_cents: 750 }), {
          status: 201,
        });
      }),
    );
    openSupport();
    const field = await screen.findByLabelText('Other amount (USD)');
    const donate = screen.getByRole('button', { name: 'Donate' });
    await userEvent.type(field, '0.50');
    expect(await screen.findByRole('alert')).toHaveTextContent('Use an amount from $1.00');
    expect(donate).toBeDisabled();
    await userEvent.clear(field);
    await userEvent.type(field, '7.50');
    expect(donate).toBeEnabled();
    await userEvent.click(donate);
    await screen.findByRole('dialog', { name: 'Pay with Bitcoin' });
    expect(calls).toEqual([{ purpose: 'donate', usd_cents: 750 }]);
  });

  it('lets a member support one of their servers anonymously', async () => {
    supportApi();
    useStore.setState({
      guilds: { g7: { id: 'g7', name: 'Book Club' } },
      guildOrder: ['g7'],
    });
    let body: unknown;
    server.use(
      http.post('*/api/v1/donations/invoices', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(invoice({ purpose: 'server_level', months: null }), {
          status: 201,
        });
      }),
    );
    openSupport();
    const select = await screen.findByLabelText('Server');
    expect(within(select).getByRole('option', { name: 'Book Club' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('switch', { name: /Contribute anonymously/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Support $10.00' }));
    await screen.findByRole('dialog', { name: 'Pay with Bitcoin' });
    expect(body).toEqual({
      purpose: 'server_level',
      guild_id: 'g7',
      usd_cents: 1000,
      anonymous: true,
    });
  });

  it('shows Patron status with the end date, and the grace period', async () => {
    const until = Date.parse('2030-05-01T12:00:00Z');
    supportApi({
      me: { ...ME, patron: { ...ME.patron, active: true, until_ms: until, started_ms: 1 } },
    });
    openSupport();
    expect(
      await screen.findByText(`You are a Patron until ${new Date(until).toLocaleDateString()}.`),
    ).toBeInTheDocument();
  });

  it('saves leaderboard privacy choices', async () => {
    supportApi();
    let patch: unknown;
    server.use(
      http.patch('*/api/v1/donations/me/preferences', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({
          hide_name: true,
          hide_amount: false,
          hide_badge: false,
          hide_contributions: false,
        });
      }),
    );
    openSupport();
    await userEvent.click(await screen.findByRole('switch', { name: /Hide my name/ }));
    await waitFor(() => expect(patch).toEqual({ hide_name: true }));
    expect(screen.getByRole('link', { name: 'View leaderboards' })).toHaveAttribute(
      'href',
      '/support/leaderboards',
    );
  });

  it('lets a contributor leave the contributor boards', async () => {
    supportApi();
    let patch: unknown;
    server.use(
      http.patch('*/api/v1/donations/me/preferences', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({
          hide_name: false,
          hide_amount: false,
          hide_badge: false,
          hide_contributions: true,
        });
      }),
    );
    openSupport();
    await userEvent.click(
      await screen.findByRole('switch', { name: /Keep me off the contributor boards/ }),
    );
    await waitFor(() => expect(patch).toEqual({ hide_contributions: true }));
  });

  it('lists the history and opens a receipt for a paid invoice', async () => {
    const paid = invoice({
      id: '77',
      state: 'confirmed',
      received_sats: 7693,
      confirmed_sats: 7693,
      credited_at_ms: Date.now(),
      explorer_url: 'https://mempool.space/tx/abcd',
      txs: [{ txid: 'abcd0000abcd0000abcd', amount_sats: 7693, confirmations: 3, dropped: false }],
    });
    supportApi({ invoices: [paid, invoice({ id: '78', state: 'expired' })] });
    openSupport();
    await userEvent.click(await screen.findByRole('button', { name: /Receipt\s+invoice 77/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Payment received' });
    expect(within(dialog).getByText('#77')).toBeInTheDocument();
    expect(within(dialog).getByRole('link', { name: 'View on a block explorer' })).toHaveAttribute(
      'href',
      'https://mempool.space/tx/abcd',
    );
  });

  it('parses dollar amounts strictly', () => {
    expect(parseDollars('5')).toBe(500);
    expect(parseDollars('$7.5')).toBe(750);
    expect(parseDollars('12.34')).toBe(1234);
    expect(parseDollars('')).toBeNull();
    expect(parseDollars('abc')).toBeNull();
    expect(parseDollars('1.234')).toBeNull();
    expect(parseDollars('0')).toBeNull();
    expect(perkLines(CONFIG.levels[0]!.perks)).toContain('8 video senders per voice channel');
  });
});

function renderModal(inv: Invoice, onClose = () => undefined) {
  return render(
    <ThemeProvider section="lounge">
      <ToastProvider>
        <InvoiceModal invoice={inv} onClose={onClose} />
      </ToastProvider>
    </ThemeProvider>,
  );
}

describe('Invoice dialog live status', () => {
  it('follows SR_PAYMENT_UPDATE from the gateway and shows the receipt', async () => {
    let state: Invoice = invoice();
    server.use(http.get('*/api/v1/donations/invoices/9001', () => HttpResponse.json(state)));
    renderModal(state);
    expect(screen.getByText('Waiting for payment', { selector: 'span' })).toBeInTheDocument();
    state = invoice({
      state: 'confirming',
      received_sats: 7693,
      confirmations: 1,
      confirmations_required: 3,
    });
    act(() =>
      emitDispatch('SR_PAYMENT_UPDATE', {
        type: 'invoice',
        invoice_id: '9001',
        state: 'confirming',
      }),
    );
    expect(await screen.findByText(/Confirming \(1 of 3\)/)).toBeInTheDocument();
    // Another invoice's event is ignored.
    state = invoice({ state: 'confirmed' });
    act(() => emitDispatch('SR_PAYMENT_UPDATE', { invoice_id: '1234' }));
    expect(screen.queryByText('Payment received')).toBeNull();
    act(() => emitDispatch('SR_PAYMENT_UPDATE', { invoice_id: '9001' }));
    expect(await screen.findByRole('dialog', { name: 'Payment received' })).toBeInTheDocument();
  });

  it('falls back to polling when no event arrives', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      let state: Invoice = invoice();
      let polls = 0;
      server.use(
        http.get('*/api/v1/donations/invoices/9001', () => {
          polls++;
          return HttpResponse.json(state);
        }),
      );
      renderModal(state);
      state = invoice({ state: 'seen', received_sats: 7693, confirmations: 0 });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(15_500);
      });
      await waitFor(() => expect(polls).toBeGreaterThan(0));
      expect(await screen.findByText('Payment seen, waiting for a block')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it('"I\'ve paid" asks the server to check now', async () => {
    let checked = 0;
    server.use(
      http.post('*/api/v1/donations/invoices/9001/check', () => {
        checked++;
        return HttpResponse.json(invoice({ state: 'seen', received_sats: 7693, confirmations: 0 }));
      }),
    );
    renderModal(invoice());
    await userEvent.click(screen.getByRole('button', { name: "I've paid" }));
    await waitFor(() => expect(checked).toBe(1));
    expect(await screen.findByText('Payment seen, waiting for a block')).toBeInTheDocument();
  });

  it('explains underpaid, overpaid, late and expired invoices', async () => {
    const { unmount } = renderModal(
      invoice({ state: 'underpaid', received_sats: 3000, shortfall_sats: 4693 }),
    );
    expect(screen.getByText('Not enough was received')).toBeInTheDocument();
    expect(screen.getByText(/Send 4,693 sats more/)).toBeInTheDocument();
    unmount();
    const o = renderModal(invoice({ state: 'overpaid', excess_sats: 500, confirmed_sats: 8193 }));
    expect(screen.getByText('You sent more than needed')).toBeInTheDocument();
    o.unmount();
    const l = renderModal(invoice({ state: 'late_paid', late: true, confirmed_sats: 7693 }));
    expect(screen.getByText('It arrived after the invoice expired')).toBeInTheDocument();
    l.unmount();
    renderModal(invoice({ state: 'expired' }));
    expect(screen.getByText('No payment arrived in time')).toBeInTheDocument();
  });

  it('cancels an unpaid invoice', async () => {
    server.use(
      http.post('*/api/v1/donations/invoices/9001/cancel', () =>
        HttpResponse.json(invoice({ state: 'cancelled' })),
      ),
    );
    renderModal(invoice());
    await userEvent.click(screen.getByRole('button', { name: 'Cancel invoice' }));
    expect(await screen.findByText('Cancelled', { selector: 'span' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Cancel invoice' })).toBeNull();
  });
});

describe('Support: perks table, contributions, explorer links (LNG-USET-031..033)', () => {
  it('compares free and Patron perks row by row', async () => {
    supportApi();
    openSupport();
    const table = await screen.findByRole('table', { name: 'Free compared with Patron' });
    const row = (name: string) => within(table).getByRole('row', { name: new RegExp(name) });
    expect(row('Upload size per file')).toHaveTextContent('30 MB');
    expect(row('Upload size per file')).toHaveTextContent('500 MB');
    expect(row('Video and screen share')).toHaveTextContent('720p30');
    expect(row('Video and screen share')).toHaveTextContent('1080p60');
    // Patron has no value of its own here: the free one still applies.
    expect(row('Viewers per stream')).toHaveTextContent('Same as free');
    expect(row('Cinema downloads')).toHaveTextContent('Not included');
    expect(row('Cinema downloads')).toHaveTextContent('20 a day');
  });

  it('lists only paid server contributions and links a transaction to the explorer', async () => {
    useStore.setState({ guilds: { '55': { id: '55', name: 'Book Club' } }, guildOrder: ['55'] });
    supportApi({
      invoices: [
        invoice({
          id: '1',
          purpose: 'server_level',
          guild_id: '55',
          state: 'confirmed',
          usd_cents: 1000,
          explorer_url: 'https://explorer.example/tx/abc',
        }),
        invoice({ id: '2', purpose: 'server_level', guild_id: '55', state: 'open' }),
        invoice({ id: '3', purpose: 'donate', state: 'confirmed' }),
      ],
    });
    openSupport();
    const card = await screen.findByRole('region', { name: 'Server contributions' });
    expect(await within(card).findByText(/Book Club, \$10\.00/)).toBeInTheDocument();
    expect(within(card).getAllByRole('listitem')).toHaveLength(1);
    const links = screen.getAllByRole('link', { name: /View transaction/ });
    expect(links[0]).toHaveAttribute('href', 'https://explorer.example/tx/abc');
  });
});

describe('/support page (DON-UI-001)', () => {
  const openPage = () => {
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/support'] }));
    render(<RouterProvider router={router} />);
    return router;
  };

  it('explains why to donate, shows the hosting bar, Patron, Server Levels, credits and the help answers', async () => {
    supportApi();
    openPage();
    expect(await screen.findByRole('heading', { name: 'Support SIDE ROOM' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Why donate' })).toBeInTheDocument();
    const bar = await screen.findByRole('progressbar', { name: /Donations this month/ });
    expect(bar).toHaveAttribute('aria-valuenow', '4200');
    expect(screen.getByText('You are not a Patron.')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Become a Patron' })).toHaveAttribute(
      'href',
      '/settings/user/support',
    );
    const levels = screen.getByRole('heading', { name: 'Server Levels' }).closest('section');
    expect(levels).toHaveTextContent('Level 2: from $25.00 given');
    expect(screen.getByRole('heading', { name: 'Invite credits' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'See the leaderboards' })).toHaveAttribute(
      'href',
      '/support/leaderboards',
    );
    // The help answers the page used to be are still on it.
    expect(screen.getByRole('heading', { name: 'Help' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /sign in/i })).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('shows a sign-in prompt, not an error, to a visitor who is not signed in', async () => {
    server.use(
      http.get('*/api/v1/donations/config', () =>
        HttpResponse.json({ code: 401, message: 'Unauthorized' }, { status: 401 }),
      ),
      http.get('*/api/v1/donations/me', () =>
        HttpResponse.json({ code: 401, message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    openPage();
    expect(await screen.findByText('Sign in to support SIDE ROOM')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Help' })).toBeInTheDocument();
  });
});
