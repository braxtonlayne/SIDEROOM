import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';

/** Default: nobody is logged in. Tests add handlers with `server.use(...)`. */
export const server = setupServer(
  http.get('*/api/v1/users/@me', () =>
    HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
  ),
  http.get('*/api/v1/sr/auth/discord/pending', () => HttpResponse.json(null)),
);

export { http, HttpResponse };
