// SEC-AUD-168: Admin > Bug reports.
import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BETA_REPORTS_FLAG } from '../../bugreport/flag';
import { setClientFlags } from '../../moderation/reports';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const T = 1_800_000_000_000;
const ROW = {
  id: '7',
  created_ms: T,
  status: 'open',
  route: 'lounge.:guildId.:channelId',
  commit: '0a1b2c3d4e5f6a7b',
  reporter: { id: '9', username: 'bea' },
  has_screenshot: true,
};
const DETAIL = {
  ...ROW,
  description: 'The channel list stays empty',
  closed_ms: null,
  closed_by: null,
  bundle: {
    commit: '0a1b2c3d4e5f6a7b',
    route: 'lounge.:guildId.:channelId',
    at: T,
    viewport: { w: 1280, h: 800, dpr: 2 },
    caps: { engine: 'chromium', wasm: true, webgpu: false },
    locale: 'en',
    theme: 'lounge-dark',
    online: true,
    gateway: { status: 'connected' },
    sw: 'active',
    voice: { connected: false },
    failed_requests: [
      { method: '?', path: '/api/v1/channels/:id/messages', status: 500, ms: 312, ago_s: 14 },
    ],
    crashes: [],
  },
};

beforeEach(() => {
  setClientFlags({ [BETA_REPORTS_FLAG]: true });
  URL.createObjectURL = vi.fn(() => 'blob:shot');
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => setClientFlags({}));

describe('Admin > Bug reports', () => {
  it('lists reports, opens one, and fetches the picture only on the click', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    let detailReads = 0;
    let shotReads = 0;
    server.use(
      http.get('*/api/v1/admin/bug-reports', () =>
        HttpResponse.json({ reports: [ROW], open_count: 1 }),
      ),
      http.get('*/api/v1/admin/bug-reports/7', () => {
        detailReads += 1;
        return HttpResponse.json(DETAIL);
      }),
      http.get('*/api/v1/admin/bug-reports/7/screenshot', () => {
        shotReads += 1;
        return new HttpResponse(new Uint8Array([255, 216, 255, 217]), {
          headers: { 'Content-Type': 'image/jpeg' },
        });
      }),
    );
    openAdmin('bug-reports');
    const table = await screen.findByRole('table', { name: 'Bug reports' });
    expect(table).toHaveTextContent('bea');
    expect(detailReads).toBe(0);
    await userEvent.click(within(table).getByRole('button', { name: /Open the report from bea/ }));
    expect(await screen.findByText('The channel list stays empty')).toBeInTheDocument();
    expect(detailReads).toBe(1);
    expect(shotReads).toBe(0);
    expect(screen.getByText('/api/v1/channels/:id/messages')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Show screenshot' }));
    expect(await screen.findByRole('img', { name: /screenshot/ })).toBeInTheDocument();
    expect(shotReads).toBe(1);
    await userEvent.click(screen.getByRole('button', { name: 'Hide screenshot' }));
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:shot');
  });

  it('closing asks first and says the picture is deleted', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    let closes = 0;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.get('*/api/v1/admin/bug-reports', () =>
        HttpResponse.json({ reports: [ROW], open_count: 1 }),
      ),
      http.get('*/api/v1/admin/bug-reports/7', () => HttpResponse.json(DETAIL)),
      http.post('*/api/v1/admin/bug-reports/7/close', () => {
        closes += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('bug-reports');
    await userEvent.click(await screen.findByRole('button', { name: /Open the report from bea/ }));
    await userEvent.click(await screen.findByRole('button', { name: 'Close report' }));
    const dialog = await screen.findByRole('dialog', { name: 'Close this report?' });
    expect(dialog).toHaveTextContent(/deletes its screenshot for good/);
    expect(closes).toBe(0);
    await act(async () => {
      await userEvent.click(within(dialog).getByRole('button', { name: 'Close report' }));
    });
    await vi.waitFor(() => expect(closes).toBe(1));
  });

  it('is not in the menu while the switch is off', async () => {
    setClientFlags({});
    asStaff({ perms: ['STAFF_BASIC'] });
    openAdmin('dashboard');
    await screen.findByRole('navigation', { name: /settings|admin/i });
    expect(screen.queryByText(/^Bug reports/)).toBeNull();
  });
});
