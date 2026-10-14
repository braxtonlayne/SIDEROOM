import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { usePrefsStore } from '../prefs';
import { openSettings, resetPrefs, signedIn } from './helpers';

const gs = (over = {}) => ({
  guild_id: '5',
  message_notifications: null,
  muted: false,
  mute_config: null,
  suppress_everyone: false,
  suppress_roles: false,
  suppress_highlights: false,
  mute_scheduled_events: false,
  mobile_push: true,
  ...over,
});

function guildEndpoints(onPatch?: (b: unknown) => Response | undefined) {
  server.use(
    http.get('*/api/v1/users/@me/guilds', () =>
      HttpResponse.json([{ id: '5', name: 'Book Club' }]),
    ),
    http.get('*/api/v1/users/@me/guilds/5/settings', () => HttpResponse.json(gs())),
    http.patch('*/api/v1/users/@me/guilds/5/settings', async ({ request }) => {
      const b = await request.json();
      return (
        onPatch?.(b) ??
        HttpResponse.json(
          gs({ muted: true, mute_config: { end_time: null, selected_time_window: 3600 } }),
        )
      );
    }),
  );
}

beforeEach(() => {
  resetPrefs();
  signedIn();
});
afterEach(() => vi.unstubAllGlobals());

describe('Notifications', () => {
  it('shows the account default level and saves a change straight to the server', async () => {
    guildEndpoints();
    let sent: unknown;
    server.use(
      http.get('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message_notifications: 1 }),
      ),
      http.patch('*/api/v1/users/@me/notification-defaults', async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ message_notifications: 0 });
      }),
    );
    const user = userEvent.setup();
    openSettings('notifications');
    expect(await screen.findByRole('radio', { name: 'Only @mentions' })).toBeChecked();
    await user.click(screen.getByRole('radio', { name: 'All messages' }));
    await waitFor(() => expect(sent).toEqual({ message_notifications: 0 }));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('radio', { name: 'All messages' })).toBeChecked();
  });

  it('puts the radio back and says why when the default level cannot be saved', async () => {
    guildEndpoints();
    server.use(
      http.get('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message_notifications: 1 }),
      ),
      http.patch('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message: 'Nope' }, { status: 400 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('radio', { name: 'Nothing' }));
    expect(await screen.findByText('Nope')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Only @mentions' })).toBeChecked();
  });

  it('keeps the unread, push and reaction choices in the saved preferences', async () => {
    guildEndpoints();
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('switch', { name: /Flash the page title/ }));
    await user.click(screen.getByRole('switch', { name: /Show the unread badge/ }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs).toMatchObject({ titleBlink: true, unreadBadge: false });
  });

  it('toggles sound preferences', async () => {
    guildEndpoints();
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('switch', { name: 'Message sound' }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs.soundMessage).toBe(false);
  });

  it('explains when the browser has no notification support', async () => {
    guildEndpoints();
    vi.stubGlobal('Notification', undefined);
    openSettings('notifications');
    expect(await screen.findByText(/doesn't support desktop notifications/)).toBeInTheDocument();
  });

  it('requests desktop notification permission', async () => {
    guildEndpoints();
    const request = vi.fn().mockResolvedValue('granted');
    vi.stubGlobal('Notification', { permission: 'default', requestPermission: request });
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('button', { name: 'Enable desktop notifications' }));
    expect(request).toHaveBeenCalled();
    expect(await screen.findByRole('switch', { name: 'Show desktop notifications' })).toBeChecked();
  });

  it('shows a blocked-permission hint', async () => {
    guildEndpoints();
    vi.stubGlobal('Notification', { permission: 'denied', requestPermission: vi.fn() });
    openSettings('notifications');
    expect(await screen.findByText('Blocked by the browser')).toBeInTheDocument();
  });

  it('mutes a server for an hour (PATCH with the preset window)', async () => {
    let sent: unknown;
    guildEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('combobox', { name: 'Mute Book Club' }));
    await user.click(await screen.findByRole('option', { name: 'For 1 hour' }));
    await waitFor(() =>
      expect(sent).toEqual({ muted: true, mute_config: { selected_time_window: 3600 } }),
    );
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Mute Book Club' })).toHaveTextContent(
        'For 1 hour',
      ),
    );
  });

  it('sets a per-server level', async () => {
    const sent: unknown[] = [];
    guildEndpoints((b) => {
      sent.push(b);
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('combobox', { name: 'Notifications for Book Club' }));
    await user.click(await screen.findByRole('option', { name: 'Nothing' }));
    await waitFor(() => expect(sent).toEqual([{ message_notifications: 2 }]));
  });

  it('shows a server error from a failed override', async () => {
    guildEndpoints(
      () => HttpResponse.json({ message: 'Nope.' }, { status: 403 }) as unknown as Response,
    );
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('combobox', { name: 'Mute Book Club' }));
    await user.click(await screen.findByRole('option', { name: 'For 8 hours' }));
    const row = (await screen.findByText('Book Club')).closest('li')!;
    expect(await within(row).findByRole('alert')).toHaveTextContent('Nope.');
  });

  it('handles having no servers', async () => {
    server.use(http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])));
    openSettings('notifications');
    expect(await screen.findByText("You aren't in any servers.")).toBeInTheDocument();
  });

  it('reports a failing server list', async () => {
    server.use(
      http.get('*/api/v1/users/@me/guilds', () =>
        HttpResponse.json({ message: 'x' }, { status: 500 }),
      ),
    );
    openSettings('notifications');
    expect(await screen.findByText("Couldn't load your servers")).toBeInTheDocument();
  });
});
