import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';

/** Default: nobody is logged in. Tests add handlers with `server.use(...)`. */
export const server = setupServer(
  http.get('*/api/v1/users/@me', () =>
    HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
  ),
  http.get('*/api/v1/sr/entrance/copy', () => HttpResponse.json({})),
  http.get('*/api/v1/sr/emojis/:id/url', () =>
    HttpResponse.json({ message: 'Unknown emoji' }, { status: 404 }),
  ),
  http.get('*/api/v1/sr/auth/discord/pending', () => HttpResponse.json(null)),
  http.get('*/api/v1/sr/auth/passkeys', () => HttpResponse.json([])),
  // Home screen loads: nobody to befriend, no templates, online with no custom status.
  http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
  http.get('*/api/v1/guilds/templates/builtin', () => HttpResponse.json([])),
  // Apps: no commands anywhere, and an interaction is accepted (the app never answers).
  http.get('*/api/v1/channels/*/application-commands', () =>
    HttpResponse.json({ applications: [], application_commands: [] }),
  ),
  http.post('*/api/v1/interactions', () => new HttpResponse(null, { status: 204 })),
  http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/presence', () =>
    HttpResponse.json({ status: 'online', custom_status: null }),
  ),
  // The synced settings blob starts empty (nothing saved yet).
  http.get('*/api/v1/users/@me/settings', () => HttpResponse.json({ version: 0, settings: {} })),
  http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
    const body = (await request.json()) as { settings: unknown };
    return HttpResponse.json({ version: 1, settings: body.settings });
  }),
  // Settings screens: privacy rules and the global default notification level start at the server defaults.
  http.get('*/api/v1/users/@me/privacy', () =>
    HttpResponse.json({
      friend_requests_everyone: true,
      friend_requests_friends_of_friends: true,
      friend_requests_server_members: true,
      allow_server_member_dms: true,
      message_requests: true,
      sensitive_media: { friends: 'show', others: 'blur', servers: 'show' },
      dm_spam_filter: 'non_friends',
      server_dm_overrides: {},
    }),
  ),
  http.get('*/api/v1/users/@me/age-confirmation', () =>
    HttpResponse.json({ confirmed_at: null, method: null }),
  ),
  http.get('*/api/v1/users/@me/notification-keywords', () => HttpResponse.json({ keywords: [] })),
  http.get('*/api/v1/users/@me/notification-defaults', () =>
    HttpResponse.json({ message_notifications: null }),
  ),
  // Saved items (bookmarks, reminders, scheduled messages) start empty.
  http.get('*/api/v1/users/@me/bookmarks', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/reminders', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/scheduled-messages', () => HttpResponse.json([])),
  http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'test' })),
  // The Lounge loads notification settings for every server; default to "nothing muted".
  http.get('*/api/v1/users/@me/guilds/:id/settings', () =>
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

export { http, HttpResponse };
