import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, calls, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

async function openRailMenu() {
  const rail = await screen.findByRole('navigation', { name: 'Servers' });
  const icon = await within(rail).findByRole('link', { name: /Store Guild/ });
  fireEvent.contextMenu(icon);
  return screen.findByRole('menu');
}

describe('LNG-RAIL-012 guild context menu', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/settings', () =>
        HttpResponse.json({
          muted: false,
          mute_config: null,
          message_notifications: null,
          suppress_everyone: false,
          suppress_roles: false,
          channel_overrides: [],
        }),
      ),
    );
  });

  it('lists the Discord entries, with Server Settings shortcuts', async () => {
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, developerMode: true } });
    at('/channels/g1/c1');
    const menu = await openRailMenu();
    for (const name of [
      'Invite People',
      'Mute Server',
      'Notification Settings',
      'Server Settings',
      'Privacy Settings',
      'Edit Per-server Profile',
      'Create Channel',
      'Create Category',
      'Create Event',
      'Copy Server ID',
    ])
      expect(within(menu).getByRole('menuitem', { name })).toBeTruthy();
    expect(
      within(menu).getByRole('menuitemcheckbox', { name: 'Hide Muted Channels' }),
    ).toBeTruthy();
    await userEvent.hover(within(menu).getByRole('menuitem', { name: 'Server Settings' }));
    expect(within(menu).getByRole('menuitem', { name: 'Create Event' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  });

  it('G-56 hides Copy Server ID until Developer Mode is on', async () => {
    at('/channels/g1/c1');
    const menu = await openRailMenu();
    expect(within(menu).getByRole('menuitem', { name: 'Invite People' })).toBeTruthy();
    expect(within(menu).queryByRole('menuitem', { name: 'Copy Server ID' })).toBeNull();
  });

  it('saves the per-server profile', async () => {
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ nick: 'Old nick', bio: 'old bio', roles: [] }),
      ),
      http.patch('*/api/v1/guilds/g1/members/@me', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json({ nick: 'New nick', bio: 'About here', roles: [] });
      }),
    );
    at('/channels/g1/c1');
    const menu = await openRailMenu();
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'Edit Per-server Profile' }));
    const dlg = await screen.findByRole('dialog', { name: 'Edit Per-server Profile' });
    const nick = within(dlg).getByLabelText('Nickname');
    await waitFor(() => expect(nick).toHaveValue('Old nick'));
    await userEvent.clear(nick);
    await userEvent.type(nick, 'New nick');
    const bio = within(dlg).getByLabelText('About me');
    await userEvent.clear(bio);
    await userEvent.type(bio, 'About here');
    await userEvent.click(within(dlg).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /guilds/g1/members/@me')?.body).toEqual({
        nick: 'New nick',
        bio: 'About here',
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Edit Per-server Profile' })).toBeNull(),
    );
  });

  it('shows the server error when saving fails', async () => {
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/member', () => HttpResponse.json({ roles: [] })),
      http.patch('*/api/v1/guilds/g1/members/@me', () =>
        HttpResponse.json({ message: 'Nickname is not allowed.' }, { status: 400 }),
      ),
    );
    at('/channels/g1/c1');
    const menu = await openRailMenu();
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'Edit Per-server Profile' }));
    const dlg = await screen.findByRole('dialog', { name: 'Edit Per-server Profile' });
    await userEvent.type(within(dlg).getByLabelText('Nickname'), 'x');
    await userEvent.click(within(dlg).getByRole('button', { name: 'Save' }));
    expect(await within(dlg).findByRole('alert')).toBeInTheDocument();
  });
});
