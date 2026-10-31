// BRG-COR-001: the Discord bridge tile on Admin > System (servers, maps, the queue and the dead-letter counter).
import { screen, within } from '@testing-library/react';
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
};

describe('the Discord bridge tile', () => {
  it('shows the counters and the dead-letter number', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({
          ...SYSTEM,
          bridge: { platform_on: true, bridges: 2, maps: 5, queued: 3, dead_letters: 1 },
        }),
      ),
    );
    openAdmin('system');
    const tile = await screen.findByRole('list', { name: 'Discord bridge' });
    expect(within(tile).getByText('2 servers connected')).toBeInTheDocument();
    expect(within(tile).getByText(/5 mapped channels/)).toBeInTheDocument();
    expect(within(tile).getByText(/3 posts waiting/)).toBeInTheDocument();
    expect(within(tile).getByText(/1 failed for good/)).toBeInTheDocument();
  });

  it('says so when the operator switched the bridge off, and is absent when it is not wired', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({
          ...SYSTEM,
          bridge: { platform_on: false, bridges: 0, maps: 0, queued: 0, dead_letters: 0 },
        }),
      ),
    );
    openAdmin('system');
    expect(await screen.findByText('Switched off by the operator')).toBeInTheDocument();
  });

  it('shows no tile without bridge figures', async () => {
    asStaff({ perms: [], owner: true });
    server.use(http.get('*/api/v1/admin/system', () => HttpResponse.json(SYSTEM)));
    openAdmin('system');
    await screen.findByRole('heading', { name: 'System' });
    expect(screen.queryByRole('list', { name: 'Discord bridge' })).not.toBeInTheDocument();
  });
});
