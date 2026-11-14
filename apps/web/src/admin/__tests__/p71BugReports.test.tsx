// SEC-AUD-266 and 267: a report with parts missing opens, and the list pages.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BETA_REPORTS_FLAG } from '../../bugreport/flag';
import { setClientFlags } from '../../moderation/reports';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const T = 1_800_000_000_000;
const row = (n: number, status = 'open') => ({
  id: String(n),
  created_ms: T - n * 1000,
  status,
  route: 'lounge.:guildId.:channelId',
  commit: '0a1b2c3d4e5f6a7b',
  reporter: { id: '9', username: `user${n}` },
  has_screenshot: false,
});
const FULL_BUNDLE = {
  commit: '0a1b2c3d4e5f6a7b',
  route: 'lounge.:guildId.:channelId',
  at: T,
  viewport: { w: 1280, h: 800, dpr: 2 },
  caps: { engine: 'chromium', wasm: true },
  locale: 'en',
  theme: 'lounge-dark',
  online: true,
  gateway: { status: 'connected' },
  sw: 'active',
  voice: { connected: false },
  failed_requests: [],
  crashes: [],
};
const detail = (bundle: Record<string, unknown>) => ({
  ...row(7),
  description: 'The channel list stays empty',
  closed_ms: null,
  closed_by: null,
  bundle,
});

beforeEach(() => {
  setClientFlags({ [BETA_REPORTS_FLAG]: true });
  URL.createObjectURL = vi.fn(() => 'blob:shot');
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => setClientFlags({}));

async function openReport(bundle: Record<string, unknown>) {
  asStaff({ perms: ['STAFF_BASIC'] });
  server.use(
    http.get('*/api/v1/admin/bug-reports', () =>
      HttpResponse.json({ reports: [row(7)], open_count: 1, has_more: false }),
    ),
    http.get('*/api/v1/admin/bug-reports/7', () => HttpResponse.json(detail(bundle))),
  );
  openAdmin('bug-reports');
  await userEvent.click(await screen.findByRole('button', { name: /Open the report from user7/ }));
  return screen.findByText('The channel list stays empty');
}

describe('a report with parts missing (SEC-AUD-266)', () => {
  it('opens a report stored with only commit, route and time, and keeps its Close button', async () => {
    // The shape the server stored before SEC-AUD-266 for a body of only a description: the page threw on viewport.w.
    await openReport({ commit: '0a1b2c3d4e5f6a7b', route: 'unknown', at: T });
    const debug = screen.getByRole('region', { name: /debug/i });
    expect(within(debug).getAllByText('Unknown').length).toBeGreaterThanOrEqual(6);
    expect(screen.getByRole('button', { name: 'Close report' })).toBeInTheDocument();
  });

  it('opens a report whose parts hold the neutral values the server writes now', async () => {
    await openReport({
      ...FULL_BUNDLE,
      viewport: { w: 0, h: 0, dpr: 0 },
      gateway: { status: 'unknown' },
      voice: { connected: false },
      sw: 'none',
    });
    const debug = screen.getByRole('region', { name: /debug/i });
    expect(within(debug).getAllByText('Unknown').length).toBeGreaterThanOrEqual(2);
    expect(within(debug).queryByText(/0 × 0/)).toBeNull();
  });

  it('shows a full report as before', async () => {
    await openReport(FULL_BUNDLE);
    const debug = screen.getByRole('region', { name: /debug/i });
    expect(within(debug).getByText('1280 × 800 @2')).toBeInTheDocument();
    expect(within(debug).getByText('chromium')).toBeInTheDocument();
    expect(within(debug).getByText('connected')).toBeInTheDocument();
    expect(within(debug).queryByText('Unknown')).toBeNull();
  });
});

describe('the list pages (SEC-AUD-267)', () => {
  it('"Load more" appends the next page after the last id, and the status filter starts again', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    const asked: string[] = [];
    server.use(
      http.get('*/api/v1/admin/bug-reports', ({ request }) => {
        const u = new URL(request.url);
        // The Admin shell's own badge asks for one report; only the page's asks count here.
        if (u.searchParams.get('limit') === '50') {
          asked.push(`${u.searchParams.get('status')}:${u.searchParams.get('before') ?? '-'}`);
        }
        const status = u.searchParams.get('status') ?? 'open';
        if (status === 'closed') {
          return HttpResponse.json({
            reports: [row(5, 'closed')],
            open_count: 60,
            has_more: false,
          });
        }
        if (u.searchParams.get('before') === '11') {
          return HttpResponse.json({
            reports: [row(10), row(9)],
            open_count: 60,
            has_more: false,
          });
        }
        return HttpResponse.json({
          reports: [row(12), row(11)],
          open_count: 60,
          has_more: true,
        });
      }),
    );
    openAdmin('bug-reports');
    const table = await screen.findByRole('table', { name: 'Bug reports' });
    expect(within(table).getAllByRole('row')).toHaveLength(3);
    await userEvent.click(screen.getByRole('button', { name: 'Load more' }));
    await vi.waitFor(() => expect(within(table).getAllByRole('row')).toHaveLength(5));
    expect(within(table).getByText('user9')).toBeInTheDocument();
    expect(asked).toEqual(['open:-', 'open:11']);
    // No more pages: no button.
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();
    // Another status lists that status from its newest report.
    await userEvent.click(screen.getByRole('combobox', { name: 'Show reports that are' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Closed' }));
    await vi.waitFor(() => expect(asked.at(-1)).toBe('closed:-'));
    await vi.waitFor(() =>
      expect(
        within(screen.getByRole('table', { name: 'Bug reports' })).getAllByRole('row'),
      ).toHaveLength(2),
    );
    expect(screen.queryByText('user9')).toBeNull();
  });

  it('says so when the next page cannot be loaded, and keeps the rows it has', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(
      http.get('*/api/v1/admin/bug-reports', ({ request }) =>
        new URL(request.url).searchParams.has('before')
          ? new HttpResponse(null, { status: 500 })
          : HttpResponse.json({ reports: [row(12)], open_count: 1, has_more: true }),
      ),
    );
    openAdmin('bug-reports');
    await screen.findByRole('table', { name: 'Bug reports' });
    await userEvent.click(screen.getByRole('button', { name: 'Load more' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not load more reports');
    expect(screen.getByText('user12')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Load more' })).toBeInTheDocument();
  });
});

describe('a report whose browser said nothing about being online (SEC-AUD-277, D-758)', () => {
  const onlineCell = () => {
    const debug = screen.getByRole('region', { name: /debug/i });
    return within(debug).getByText('Online').nextElementSibling as HTMLElement;
  };

  it('reads "Unknown" for online: null and for a row without the key', async () => {
    await openReport({ ...FULL_BUNDLE, online: null });
    expect(onlineCell()).toHaveTextContent('Unknown');
  });

  it('reads "Unknown" for a row stored before the key existed', async () => {
    const { online: _gone, ...old } = FULL_BUNDLE;
    void _gone;
    await openReport(old);
    expect(onlineCell()).toHaveTextContent('Unknown');
  });

  it('still reads Yes and No for true and false', async () => {
    await openReport({ ...FULL_BUNDLE, online: true });
    expect(onlineCell()).toHaveTextContent('Yes');
  });

  it('reads No for false', async () => {
    await openReport({ ...FULL_BUNDLE, online: false });
    expect(onlineCell()).toHaveTextContent('No');
  });
});
