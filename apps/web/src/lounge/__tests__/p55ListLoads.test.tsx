// SEC-AUD-046 (D-577): a failed channel-list or member-list load is shown where the list is, with Try again,
// and retried when the browser comes back online; it never raises a toast (a toast is for the member's action).
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MessageKey } from '../../i18n';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { onActionError } from '../actionError';
import { resetLoadFailures, useLoadFailures } from '../loadFailures';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

warmLoungeRoute();

const fail500 = () => HttpResponse.json({ message: 'boom' }, { status: 500 });

let reported: MessageKey[];
let unsubscribe: () => void;
let up: boolean;
beforeEach(() => {
  baseHandlers();
  resetLoadFailures();
  reported = [];
  unsubscribe = onActionError((k) => reported.push(k));
  up = false;
  server.use(
    http.get('*/api/v1/guilds/g1', () =>
      up ? HttpResponse.json({ id: 'g1', name: 'Store Guild', owner_id: '1' }) : fail500(),
    ),
    http.get('*/api/v1/guilds/g1/channels', () =>
      up
        ? HttpResponse.json([
            { id: 'c1', type: 0, guild_id: 'g1', name: 'general', position: 1 },
            { id: 'c2', type: 0, guild_id: 'g1', name: 'random', position: 2 },
          ])
        : fail500(),
    ),
    http.get('*/api/v1/guilds/g1/roles', () =>
      up
        ? HttpResponse.json([
            { id: 'g1', name: '@everyone', color: 0, position: 0, permissions: '3072' },
          ])
        : fail500(),
    ),
    http.get('*/api/v1/guilds/g1/members', () =>
      up
        ? HttpResponse.json([{ user: { id: '2', username: 'bob', global_name: 'Bob' }, roles: [] }])
        : fail500(),
    ),
  );
});
afterEach(() => unsubscribe());

/** The guild is known from READY, its channels and members are not loaded yet. */
function seedStub() {
  seedLounge({ messages: [] });
  useStore.setState((s) => {
    const channelIdsByGuild = { ...s.channelIdsByGuild };
    const memberIdsByGuild = { ...s.memberIdsByGuild };
    delete channelIdsByGuild.g1;
    delete memberIdsByGuild.g1;
    return { channelIdsByGuild, memberIdsByGuild };
  });
}

describe('list loads (SEC-AUD-046, D-577)', () => {
  it('a failed channel list says so where the list is, with Try again, and no toast', async () => {
    seedStub();
    const user = userEvent.setup();
    at('/channels/g1/c1');
    expect(await screen.findByText('The channels could not be loaded.')).toBeInTheDocument();
    expect(screen.queryByText('No channels yet.')).toBeNull();
    expect(reported).toEqual([]);
    up = true;
    const sidebar = screen.getByTestId('channel-sidebar');
    await user.click(
      sidebar.querySelector<HTMLButtonElement>('[data-testid="list-load-failed"] button')!,
    );
    await waitFor(() => expect(useStore.getState().channelIdsByGuild.g1).toBeDefined());
    expect(screen.queryByText('The channels could not be loaded.')).toBeNull();
    expect(reported).toEqual([]);
  });

  it('a failed member list says so and is loaded again when the browser comes back online', async () => {
    seedStub();
    at('/channels/g1/c1');
    expect(await screen.findByText('The member list could not be loaded.')).toBeInTheDocument();
    expect(reported).toEqual([]);
    // Not retried in a loop while it stays down.
    expect(Object.keys(useLoadFailures.getState().failed)).toContain('members:g1');
    up = true;
    act(() => {
      window.dispatchEvent(new Event('online'));
    });
    await waitFor(() => expect(useStore.getState().memberIdsByGuild.g1).toBeDefined());
    await waitFor(() =>
      expect(screen.queryByText('The member list could not be loaded.')).toBeNull(),
    );
    expect(reported).toEqual([]);
  });
});
