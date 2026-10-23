// RDY-OPS-005: the Preflight card on Admin > System and as the last step of the Setup checklist.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const COUNTS = {
  accounts_active: 7,
  accounts_banned: 0,
  accounts_deactivated: 1,
  accounts_deleted: 0,
  jobs_dead: 0,
  jobs_queued: 0,
  jobs_running: 0,
  sessions_active: 4,
  staff: 1,
};
const SYSTEM = {
  counts: COUNTS,
  gateway_sessions: 2,
  probes: [{ name: 'database', ok: true }],
  version: '1',
  integrations: [],
};
const REPORT = {
  checks: [
    { id: 'database', title: 'Database', status: 'pass', detail: 'reachable' },
    { id: 'turn', title: 'TURN', status: 'warn', detail: 'no TURN configured' },
    {
      id: 'storage',
      title: 'Object storage',
      status: 'fail',
      detail: 'put failed: bucket missing',
    },
  ],
  ran_at_ms: 1_800_000_000_000,
  passed: 1,
  warned: 1,
  failed: 1,
  ok: false,
};

describe('preflight card', () => {
  it('runs the checks from Admin > System and shows a row per check', async () => {
    asStaff({ perms: [], owner: true });
    let runs = 0;
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.post('*/api/v1/admin/system/preflight', () => {
        runs += 1;
        return HttpResponse.json(REPORT);
      }),
    );
    openAdmin('system');
    expect(await screen.findByText('Not run yet.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Run preflight' }));
    const list = await screen.findByRole('list', { name: 'Preflight results' });
    const rows = within(list).getAllByRole('listitem');
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText('Pass')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('Warning')).toBeInTheDocument();
    expect(within(rows[2]!).getByText('Fail')).toBeInTheDocument();
    expect(within(rows[2]!).getByText('put failed: bucket missing')).toBeInTheDocument();
    expect(screen.getByText(/1 check passed/)).toHaveTextContent(
      '1 check passed, 1 warning, 1 failed. Not ready: fix the failing checks before the first invite.',
    );
    expect(runs).toBe(1);
  });

  it('says ready when nothing failed and shows a failed request as an error', async () => {
    asStaff({ perms: [], owner: true });
    let first = true;
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.post('*/api/v1/admin/system/preflight', () => {
        if (first) {
          first = false;
          return HttpResponse.json({ code: 900001, message: 'broken' }, { status: 500 });
        }
        return HttpResponse.json({
          ...REPORT,
          checks: [REPORT.checks[0]],
          passed: 1,
          warned: 0,
          failed: 0,
          ok: true,
        });
      }),
    );
    openAdmin('system');
    const run = await screen.findByRole('button', { name: 'Run preflight' });
    await userEvent.click(run);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Run preflight' }));
    expect(await screen.findByText(/Ready: nothing failed/)).toHaveTextContent(
      'Ready: nothing failed.',
    );
  });

  it('is the last step of the Setup checklist for operators only', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(COUNTS)),
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.get('*/api/v1/admin/settings', () => HttpResponse.json([])),
      http.post('*/api/v1/admin/system/preflight', () => HttpResponse.json(REPORT)),
    );
    openAdmin('dashboard');
    expect(
      await screen.findByRole('heading', { name: 'Final step: preflight' }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Run preflight' }));
    expect(await screen.findByRole('list', { name: 'Preflight results' })).toBeInTheDocument();
  });
});
