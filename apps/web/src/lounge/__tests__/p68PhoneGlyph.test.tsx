// SEC-AUD-206 (D-702): the member list draws the phone glyph and its name for somebody online on a phone alone.
import { screen, waitFor, within } from '@testing-library/react';
import { useStore } from '../../store';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

warmLoungeRoute();

describe('the phone glyph in the member list', () => {
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [msg('100')] });
  });

  const row = async (name: RegExp) => {
    const list = await screen.findByRole('list', { name: 'Members' });
    await waitFor(() => expect(list.querySelectorAll('.lng-member').length).toBeGreaterThan(1));
    return within(list).getByRole('button', { name });
  };

  it('replaces the dot with the glyph and names it, and the dot comes back with a web session', async () => {
    useStore.setState({
      presences: {
        '2': {
          user_id: '2',
          status: 'online',
          activities: [],
          client_status: { mobile: 'online' },
        },
      },
    });
    at('/channels/g1/c1');
    const bobRow = await row(/Bob/);
    expect(within(bobRow).getByRole('img', { name: 'Online on a phone' })).toBeInTheDocument();
    expect(within(bobRow).queryByRole('img', { name: 'Online' })).toBeNull();
    expect(bobRow.getAttribute('aria-label')).toContain('online on a phone');
    expect(bobRow.querySelector('[data-icon="phone"]')).not.toBeNull();

    useStore.setState({
      presences: {
        '2': {
          user_id: '2',
          status: 'online',
          activities: [],
          client_status: { mobile: 'online', web: 'online' },
        },
      },
    });
    await waitFor(() =>
      expect(within(bobRow).getByRole('img', { name: 'Online' })).toBeInTheDocument(),
    );
    expect(bobRow.querySelector('[data-icon="phone"]')).toBeNull();
  });
});
