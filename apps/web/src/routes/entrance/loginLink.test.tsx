import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('owner login link page (/login/link?token=)', () => {
  let posted: unknown[];

  beforeEach(() => {
    posted = [];
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf-test' })));
  });

  it('posts the token once and continues to the Lounge', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const router = at('/login/link?token=abc123');
    // The Lounge may land on the remembered location under /channels (Q166).
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/channels/));
    expect(posted).toEqual([{ token: 'abc123' }]);
  });

  it('shows progress while the exchange is in flight', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', async () => {
        await new Promise((resolve) => setTimeout(resolve, 300));
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const router = at('/login/link?token=abc123');
    expect(await screen.findByRole('status')).toHaveTextContent('One moment…');
    // Let the exchange finish before the test ends: otherwise its navigation lands after the environment is
    // torn down ("window is not defined", an unhandled rejection that fails the CI run).
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/channels/), {
      timeout: 10_000,
    });
  });

  it.each([
    [400, 'This link is invalid, has expired, or was already used.'],
    [401, 'This link is invalid, has expired, or was already used.'],
  ])('explains a rejected link (HTTP %i)', async (status, text) => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', () =>
        HttpResponse.json({ message: 'This link is invalid or has expired.' }, { status }),
      ),
    );
    const router = at('/login/link?token=stale');
    expect(await screen.findByRole('alert')).toHaveTextContent(text);
    expect(router.state.location.pathname).toBe('/login/link');
  });

  it('asks the owner to wait when the endpoint throttles them', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', () =>
        HttpResponse.json({ message: 'slow down', retry_after: 120 }, { status: 429 }),
      ),
    );
    at('/login/link?token=abc123');
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Too many tries. Try again in 2 minutes.',
    );
  });

  it('shows the network message when the request never completes', async () => {
    server.use(http.post('*/api/v1/sr/auth/login-link', () => HttpResponse.error()));
    at('/login/link?token=abc123');
    expect(await screen.findByRole('alert')).toHaveTextContent(
      "We couldn't reach the door. Check your connection and try again.",
    );
  });

  it('sends nothing and explains the problem when the token is missing', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    at('/login/link');
    expect(await screen.findByRole('alert')).toHaveTextContent('missing its token');
    expect(posted).toEqual([]);
  });
});
