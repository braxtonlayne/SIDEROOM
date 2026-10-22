import { screen } from '@testing-library/react';
import { resetStore } from '../../../store';
import { setReportsEnabled } from '../../../moderation/reports';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

const SETTLED = [
  http.get('*/api/v1/users/@me/data-export', () => HttpResponse.json({ state: 'none' })),
  http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
];

beforeEach(() => {
  resetStore();
  resetPrefs();
  signedIn();
  setReportsEnabled(false);
});
afterEach(() => setReportsEnabled(false));

const REPORT = {
  id: '1',
  target_kind: 'message',
  target_id: '5',
  category: 'harassment',
  state: 'closed',
  outcome_line: 'Thank you, nothing broke the rules.',
  created_at_ms: 1_700_000_000_000,
};

describe('Your reports (N-24)', () => {
  it('lists the state and the outcome line, and nothing about the case', async () => {
    server.use(
      ...SETTLED,
      http.get('*/api/v1/moderation/reports/mine', () =>
        HttpResponse.json([
          REPORT,
          { ...REPORT, id: '2', target_kind: 'user', state: 'received', outcome_line: '' },
        ]),
      ),
    );
    openSettings('privacy');
    const list = await screen.findByRole('list', { name: 'Your reports' });
    expect(list).toHaveTextContent('Closed');
    expect(list).toHaveTextContent('Thank you, nothing broke the rules.');
    expect(list).toHaveTextContent('Received');
    expect(list).toHaveTextContent('Harassment, hate or threats');
  });

  it('is hidden when reporting is off and there is nothing to read', async () => {
    server.use(
      ...SETTLED,
      http.get('*/api/v1/moderation/reports/mine', () => HttpResponse.json([])),
    );
    openSettings('privacy');
    await screen.findByRole('heading', { name: 'Privacy & Safety' });
    await vi.waitFor(() => expect(screen.queryByText('Your reports')).toBeNull());
  });

  it('shows an empty card while reporting is on', async () => {
    setReportsEnabled(true);
    server.use(
      ...SETTLED,
      http.get('*/api/v1/moderation/reports/mine', () => HttpResponse.json([])),
    );
    openSettings('privacy');
    expect(await screen.findByText('You have not sent any reports.')).toBeInTheDocument();
  });
});
