import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { byosMockHandlers, mockMount, PASSING_PROBES } from '../storage/mocks';
import type { ProbeRow, TestResult } from '../storage/types';
import { openSettings, resetPrefs, signedIn } from './helpers';

// SEC-AUD-299 (D-795): the test table leaves out its `cors` row once a CORS check or Apply CORS has
// answered after the test ran. The mock answers `checked_at` / `cors_ok_at` = 2026-10-26T12:00:00Z.

const WARN_MESSAGE = 'The bucket has no CORS rules yet. Use Apply CORS below.';
const WARN_ROWS: ProbeRow[] = PASSING_PROBES.map((r) =>
  r.id === 'cors' ? { id: 'cors', status: 'warn', message: WARN_MESSAGE } : r,
);
const OLD_TEST: TestResult = { at: '2026-10-26T11:00:00.000Z', rows: WARN_ROWS };

beforeEach(() => {
  resetPrefs();
  signedIn();
});

async function table() {
  return screen.findByRole('table', { name: 'Connection test results' });
}

describe('The test table and its CORS row (SEC-AUD-299)', () => {
  it('drops the CORS row after Apply CORS answers ok, and keeps it dropped after the reload', async () => {
    const user = userEvent.setup();
    server.use(
      ...byosMockHandlers({
        mounts: [mockMount({ last_test: OLD_TEST, last_test_at: OLD_TEST.at })],
        cors: 'missing',
      }),
    );
    openSettings('storage');
    const before = await table();
    expect(within(before).getByText(WARN_MESSAGE)).toBeInTheDocument();
    expect(within(before).getAllByRole('row')).toHaveLength(7);
    await user.click(screen.getByRole('button', { name: 'Apply CORS' }));
    expect(await screen.findByText('Ready')).toBeInTheDocument();
    // The list reloaded (the badge reads `cors_ok_at` of the reloaded mount) and the row is still gone.
    await waitFor(() => expect(screen.queryByText(WARN_MESSAGE)).toBeNull());
    expect(within(await table()).getAllByRole('row')).toHaveLength(6);
  });

  it('drops the CORS row when Check CORS answers missing, and the section shows that state', async () => {
    const user = userEvent.setup();
    server.use(
      ...byosMockHandlers({
        mounts: [mockMount({ last_test: OLD_TEST, last_test_at: OLD_TEST.at })],
        cors: 'missing',
      }),
    );
    openSettings('storage');
    expect(within(await table()).getByText(WARN_MESSAGE)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Check CORS' }));
    expect(await screen.findByText(/no matching rule/)).toBeInTheDocument();
    expect(screen.queryByText(WARN_MESSAGE)).toBeNull();
    expect(within(await table()).getAllByRole('row')).toHaveLength(6);
  });

  it('brings the CORS row back, fresh, when Test connection runs again', async () => {
    const user = userEvent.setup();
    server.use(
      http.post('*/api/v1/users/@me/storage/mounts/:id/test', () =>
        HttpResponse.json({ at: '2026-10-26T12:30:00.000Z', rows: PASSING_PROBES }),
      ),
      ...byosMockHandlers({
        mounts: [mockMount({ last_test: OLD_TEST, last_test_at: OLD_TEST.at })],
        cors: 'missing',
      }),
    );
    openSettings('storage');
    await table();
    await user.click(screen.getByRole('button', { name: 'Apply CORS' }));
    expect(await screen.findByText('Ready')).toBeInTheDocument();
    await waitFor(() =>
      expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(6),
    );
    await user.click(screen.getByRole('button', { name: 'Test connection' }));
    await waitFor(() =>
      expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(7),
    );
    expect(screen.getByText('Bucket rules can be read.')).toBeInTheDocument();
  });

  it('hides the CORS row of a mount loaded with a later cors_ok_at and no local state', async () => {
    server.use(
      ...byosMockHandlers({
        mounts: [
          mockMount({
            last_test: OLD_TEST,
            last_test_at: OLD_TEST.at,
            cors_ok_at: '2026-10-26T11:30:00.000Z',
          }),
        ],
      }),
    );
    openSettings('storage');
    expect(within(await table()).getAllByRole('row')).toHaveLength(6);
    expect(screen.queryByText(WARN_MESSAGE)).toBeNull();
  });

  it('keeps the CORS row when cors_ok_at is earlier than the test', async () => {
    server.use(
      ...byosMockHandlers({
        mounts: [
          mockMount({
            last_test: OLD_TEST,
            last_test_at: OLD_TEST.at,
            cors_ok_at: '2026-10-26T10:00:00.000Z',
          }),
        ],
      }),
    );
    openSettings('storage');
    expect(within(await table()).getByText(WARN_MESSAGE)).toBeInTheDocument();
    expect(within(await table()).getAllByRole('row')).toHaveLength(7);
  });
});
