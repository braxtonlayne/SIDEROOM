import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';

/** Default: nobody is logged in. Tests add handlers with `server.use(...)`. */
export const server = setupServer(
  http.get('*/api/v1/users/@me', () =>
    HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
  ),
  http.get('*/api/v1/sr/auth/discord/pending', () => HttpResponse.json(null)),
  // Home screen loads: nobody to befriend, no DMs, online with no custom status.
  http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/channels', () => HttpResponse.json([])),
  http.get('*/api/v1/users/@me/presence', () =>
    HttpResponse.json({ status: 'online', custom_status: null }),
  ),
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
