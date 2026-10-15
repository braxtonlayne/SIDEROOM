import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { resetSupportToggle } from '../shell/useSupportEnabled';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('LNG-HOME-004 Support SIDE ROOM nav item', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    resetSupportToggle();
  });

  it('shows when the operator enabled donations and opens the Support settings', async () => {
    server.use(http.get('*/api/v1/donations/config', () => HttpResponse.json({ enabled: true })));
    const { router } = at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: 'Support SIDE ROOM' }));
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/support'));
  });

  it('is absent when donations are off or the config cannot be read', async () => {
    server.use(http.get('*/api/v1/donations/config', () => HttpResponse.json({ enabled: false })));
    const first = at('/channels/@me');
    await screen.findByRole('button', { name: 'Message Requests' });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('button', { name: 'Support SIDE ROOM' })).toBeNull();
    first.unmount();

    resetSupportToggle();
    server.use(
      http.get('*/api/v1/donations/config', () => new HttpResponse(null, { status: 503 })),
    );
    at('/channels/@me');
    await screen.findByRole('button', { name: 'Message Requests' });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('button', { name: 'Support SIDE ROOM' })).toBeNull();
  });
});
