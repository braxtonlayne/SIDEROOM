import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { useThreads } from '../threads/threadStore';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const wireThread = (id: string) => ({
  id,
  type: 11,
  guild_id: 'g1',
  parent_id: 'c1',
  name: `Planning ${id}`,
  owner_id: '2',
  message_count: 1,
  member_count: 1,
  member: {},
  thread_metadata: { archived: false, locked: false, auto_archive_duration: 1440 },
});

describe('G-31 thread route /channels/:guild/:thread', () => {
  beforeEach(() => {
    seedLounge({ messages: [msg('100')] });
    useThreads.getState().reset();
    baseHandlers();
    server.use(
      http.get('*/api/v1/channels/t1/messages', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/guilds/g1/settings', () =>
        HttpResponse.json({ muted: false, channel_overrides: [] }),
      ),
    );
  });

  it('resolves a thread known to the store to its parent, with the thread in full view', async () => {
    useThreads.getState().upsert([wireThread('t1')]);
    at('/channels/g1/t1');
    const pane = await screen.findByRole('complementary', { name: 'Thread: Planning t1' });
    expect(pane).toHaveAttribute('data-expanded');
    // The parent channel renders too (hidden by the full-view style).
    expect(await screen.findByRole('region', { name: '#general' })).toBeInTheDocument();
  });

  it('fetches a thread the store has not seen', async () => {
    server.use(http.get('*/api/v1/channels/t1', () => HttpResponse.json(wireThread('t1'))));
    at('/channels/g1/t1');
    expect(await screen.findByRole('complementary', { name: 'Thread: Planning t1' })).toBeTruthy();
  });

  it('Open full view navigates to the thread address and Back to side pane returns', async () => {
    useThreads.getState().upsert([wireThread('t1')]);
    const { router } = at('/channels/g1/c1/threads/t1');
    const pane = await screen.findByRole('complementary', { name: 'Thread: Planning t1' });
    expect(pane).not.toHaveAttribute('data-expanded');
    await userEvent.click(within(pane).getByRole('button', { name: 'Thread options' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Open full view' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/t1'));
    const full = await screen.findByRole('complementary', { name: 'Thread: Planning t1' });
    expect(full).toHaveAttribute('data-expanded');
    await userEvent.click(within(full).getByRole('button', { name: 'Close thread' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/c1'));
  });
});
