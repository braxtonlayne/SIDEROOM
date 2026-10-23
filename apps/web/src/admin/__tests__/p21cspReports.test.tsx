// RDY-SEC-009: the CSP counters card on Admin > System.
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
const SNAPSHOT = {
  directives: [{ key: 'script-src-elem', count: 1234, last_seen_ms: 1_800_000_000_000 }],
  origins: [
    { key: 'https://cdn.bad.test', count: 1000, last_seen_ms: 1_800_000_000_000 },
    { key: 'inline', count: 234, last_seen_ms: 1_800_000_000_000 },
  ],
  paths: [{ key: '/library/series/:id/read', count: 3, last_seen_ms: 1_800_000_000_000 }],
};

describe('CSP reports card', () => {
  it('lists the directive, blocked origin and page counters', async () => {
    asStaff({ perms: [], owner: true });
    let reads = 0;
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.get('*/api/v1/admin/system/csp-reports', () => {
        reads += 1;
        return HttpResponse.json(SNAPSHOT);
      }),
    );
    openAdmin('system');
    const origins = await screen.findByRole('list', { name: 'Top blocked origins' });
    const rows = within(origins).getAllByRole('listitem');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('https://cdn.bad.test');
    expect(rows[0]).toHaveTextContent('1,000 reports');
    expect(
      within(screen.getByRole('list', { name: 'By directive' })).getByText('script-src-elem'),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole('list', { name: 'By page' })).getByText('/library/series/:id/read'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Refresh counters' }));
    await screen.findByRole('list', { name: 'Top blocked origins' });
    expect(reads).toBeGreaterThan(1);
  });

  it('says so when nothing was reported', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)),
      http.get('*/api/v1/admin/system/csp-reports', () =>
        HttpResponse.json({ directives: [], origins: [], paths: [] }),
      ),
    );
    openAdmin('system');
    expect(await screen.findByText('No reports yet.')).toBeInTheDocument();
  });
});
