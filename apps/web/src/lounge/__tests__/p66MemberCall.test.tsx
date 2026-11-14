// LNG-GLD-023: "Call" in the member context menu does what the DM header's call button does (wave 66, loose).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

warmLoungeRoute();

const startDmCall = vi.hoisted(() => vi.fn());
vi.mock('../shell/dmCall', () => ({ startDmCall }));
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

async function rightClick(who: RegExp) {
  const members = await screen.findByRole('list', { name: 'Members' });
  await userEvent.pointer({
    keys: '[MouseRight]',
    target: await within(members).findByRole('button', { name: who }),
  });
}

function membersOf(bobIsBot = false) {
  const mk = (id: string, username: string, global_name: string, bot = false) => ({
    user: { id, username, global_name, ...(bot ? { bot: true } : {}) },
    roles: [],
    joined_at: '2025-01-01T00:00:00Z',
  });
  return [mk('1', 'me', 'Me Myself'), mk('2', 'bob', 'Bob', bobIsBot)];
}

describe('member menu: Call (LNG-GLD-023)', () => {
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    startDmCall.mockReset();
    server.use(http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json(membersOf())));
  });

  it('opens or creates the DM and starts the call', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/users/@me/channels', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          id: 'dm9',
          type: 1,
          recipients: [{ id: '2', username: 'bob' }],
        });
      }),
    );
    at('/channels/g1/c1');
    await rightClick(/^Bob,/);
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Call' }));
    await waitFor(() => expect(startDmCall).toHaveBeenCalledWith('dm9', 'Bob', false));
    expect(body).toEqual({ recipient_id: '2' });
  });

  it('is not offered for a bot', async () => {
    seedLounge({
      messages: [],
      users: { '2': { id: '2', username: 'bob', global_name: 'Bob', bot: true } },
    });
    server.use(http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json(membersOf(true))));
    at('/channels/g1/c1');
    await rightClick(/^Bob,/);
    await screen.findByRole('menuitem', { name: 'Message' });
    expect(screen.queryByRole('menuitem', { name: 'Call' })).toBeNull();
  });

  it('is hidden for a blocked person', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 2, since: '2025-02-03T12:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
    );
    at('/channels/g1/c1');
    await rightClick(/^Bob,/);
    await screen.findByRole('menuitem', { name: 'Profile' });
    expect(screen.queryByRole('menuitem', { name: 'Call' })).toBeNull();
  });
});
