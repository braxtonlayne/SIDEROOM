import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { setUpdateReady } from '../../pwa/state';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const announcement = (id: string, severity = 'info') => ({
  id,
  title: `Title ${id}`,
  body: `Body ${id}`,
  severity,
  send_dm: false,
  created_by: '9',
  created_at_ms: 1,
  starts_at_ms: 1,
});

describe('LNG-SHL-009 notices bar', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    setUpdateReady(null);
  });
  afterEach(() => setUpdateReady(null));

  it('shows platform announcements and remembers a dismissal', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () =>
        HttpResponse.json([announcement('a1'), announcement('a2', 'critical')]),
      ),
    );
    const first = at('/channels/g1/c1');
    const bar = await screen.findByRole('region', { name: 'Notices' });
    expect(await within(bar).findByText('Title a1')).toBeInTheDocument();
    expect(within(bar).getByText('Title a2')).toBeInTheDocument();
    // Critical announcements cannot be dismissed.
    expect(
      within(bar).queryByRole('button', { name: 'Dismiss announcement: Title a2' }),
    ).toBeNull();
    await userEvent.click(
      within(bar).getByRole('button', { name: 'Dismiss announcement: Title a1' }),
    );
    expect(within(bar).queryByText('Title a1')).toBeNull();
    first.unmount();

    // A reload: a1 stays dismissed, a2 is still there.
    at('/channels/g1/c1');
    const again = await screen.findByRole('region', { name: 'Notices' });
    expect(await within(again).findByText('Title a2')).toBeInTheDocument();
    expect(within(again).queryByText('Title a1')).toBeNull();
  });

  it('offers a reload when an update is waiting', async () => {
    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([])));
    const apply = vi.fn();
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull();
    act(() => setUpdateReady(apply));
    const bar = await screen.findByRole('region', { name: 'Notices' });
    await userEvent.click(within(bar).getByRole('button', { name: 'Reload to update' }));
    expect(apply).toHaveBeenCalledOnce();
    await userEvent.click(within(bar).getByRole('button', { name: 'Dismiss update notice' }));
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull());
  });

  it('stays quiet when the announcements route fails', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () => new HttpResponse(null, { status: 500 })),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull();
  });
});
