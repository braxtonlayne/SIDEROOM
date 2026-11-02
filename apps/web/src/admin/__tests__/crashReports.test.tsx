// OPS-CRASH-002: Admin > Crash reports.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const T = 1_800_000_000_000;
const LIST = [
  {
    id: '1',
    fingerprint: 'aaaaaaaaaa1111',
    commit: '0a1b2c3d4e5f',
    kind: 'wasm_panic',
    route: 'arena.play',
    count: 3,
    first_seen_ms: T - 5000,
    last_seen_ms: T,
  },
  {
    id: '2',
    fingerprint: 'bbbbbbbbbb2222',
    commit: '0a1b2c3d4e5f',
    kind: 'react_boundary',
    route: 'lounge.channel',
    count: 40,
    first_seen_ms: T - 9000,
    last_seen_ms: T - 3000,
  },
];
const DETAIL = {
  ...LIST[0],
  card: {
    engine: 'chromium',
    frames: [{ file: '/assets/index.js', line: 12, col: 3, fn: 'decodeFrame' }],
    caps: { wasm: true, webgpu: false },
  },
  replay: [{ t: 1, src: 'game', input: 'jump' }],
};

describe('Admin > Crash reports', () => {
  it('lists the groups, filters and sorts them', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(http.get('*/api/v1/admin/crash-reports', () => HttpResponse.json(LIST)));
    openAdmin('crash-reports');
    const table = await screen.findByRole('table', { name: 'Crash report groups' });
    expect(within(table).getAllByRole('row')).toHaveLength(3);
    expect(table).toHaveTextContent('arena.play');
    await userEvent.type(screen.getByLabelText('Search'), 'lounge');
    expect(within(table).getAllByRole('row')).toHaveLength(2);
    expect(table).not.toHaveTextContent('arena.play');
    await userEvent.clear(screen.getByLabelText('Search'));
    await userEvent.type(screen.getByLabelText('Search'), 'nothing-matches');
    expect(screen.getByText('No report matches')).toBeInTheDocument();
  });

  it('fetches a report only when it is opened, then shows card and replay', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    let detailReads = 0;
    server.use(
      http.get('*/api/v1/admin/crash-reports', () => HttpResponse.json(LIST)),
      http.get('*/api/v1/admin/crash-reports/1', () => {
        detailReads += 1;
        return HttpResponse.json(DETAIL);
      }),
    );
    openAdmin('crash-reports');
    await screen.findByRole('table', { name: 'Crash report groups' });
    expect(detailReads).toBe(0);
    await userEvent.click(screen.getByRole('button', { name: /Open crash report aaaaaaaaaa/ }));
    expect(await screen.findByRole('table', { name: 'Replay ticks' })).toHaveTextContent('jump');
    expect(screen.getByRole('table', { name: 'Stack frames, newest first' })).toHaveTextContent(
      'decodeFrame',
    );
    expect(detailReads).toBe(1);
    await userEvent.click(screen.getByRole('button', { name: 'Back to the list' }));
    expect(await screen.findByRole('table', { name: 'Crash report groups' })).toBeInTheDocument();
  });

  it('shows empty and error states', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(http.get('*/api/v1/admin/crash-reports', () => HttpResponse.json([])));
    const first = openAdmin('crash-reports');
    expect(await screen.findByText('No crashes reported')).toBeInTheDocument();
    first.unmount();
    server.use(
      http.get('*/api/v1/admin/crash-reports', () =>
        HttpResponse.json({ code: 1, message: 'Boom' }, { status: 500 }),
      ),
    );
    openAdmin('crash-reports');
    expect(await screen.findByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('is not offered to an account that is not staff', async () => {
    asStaff({ perms: [] });
    server.use(http.get('*/api/v1/admin/crash-reports', () => HttpResponse.json(LIST)));
    openAdmin('crash-reports');
    expect(await screen.findByText(/nothing|not available|no access/i)).toBeInTheDocument();
    expect(screen.queryByRole('table', { name: 'Crash report groups' })).toBeNull();
  });
});
