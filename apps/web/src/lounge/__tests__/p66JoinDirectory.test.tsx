// LNG-GLD-018: the join dialog links to the Server Directory (wave 66, loose).
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('join dialog: Browse the Server Directory (LNG-GLD-018)', () => {
  it('opens the directory in place of the join dialog', async () => {
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/directory/categories', () => HttpResponse.json(['gaming'])),
      http.get('*/api/v1/directory', () => HttpResponse.json([])),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Join a Server' }));
    await userEvent.click(screen.getByRole('button', { name: 'Browse the Server Directory' }));
    expect(await screen.findByRole('dialog', { name: /Discover|Directory/i })).toBeInTheDocument();
    expect(screen.queryByLabelText('Invite link or code')).toBeNull();
  });
});
