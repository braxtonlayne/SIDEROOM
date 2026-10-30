import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { GatewayClient } from './api/gateway';
import { createAppRouter, routePaths } from './router';
import { warmLoungeRoute } from './test/warmRoutes';

warmLoungeRoute();

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('routes', () => {
  it.each([
    ['/channels', 'Friends'],
    ['/developers', 'Build on SIDE ROOM'],
    ['/help', 'Help'],
    ['/support', 'Support SIDE ROOM'],
  ])('renders %s', async (path, heading) => {
    at(path);
    expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
  });

  it('has no board route, link or section', async () => {
    at('/board');
    await screen.findByText(/page not found/i);
    expect(screen.queryByRole('heading', { name: /board/i })).toBeNull();
  });

  // D-385: the public pages are separate builds. None of their paths is a route of the app.
  it.each(['/login', '/register', '/forgot', '/house-notes', '/no-invitation', '/join/abc'])(
    'has no entrance route %s (a public surface serves it)',
    async (path) => {
      at(path);
      await screen.findByText(/page not found/i);
    },
  );

  it('sends /app, the neutral door, to the last section', async () => {
    const router = at('/app');
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
  });

  it('opens a page of the app named by ?goto=', async () => {
    const router = at('/app?goto=%2Fsupport');
    await waitFor(() => expect(router.state.location.pathname).toBe('/support'));
  });

  it('lists its paths for the isolation check (routes.json)', () => {
    const paths = routePaths();
    expect(paths).toEqual(expect.arrayContaining(['/', '/app', '/channels', '/help']));
    expect(paths).not.toContain('/login');
    expect(paths).not.toContain('/house-notes');
  });
});

describe('GatewayClient', () => {
  it('caps backoff', () => {
    expect(GatewayClient.backoffMs(0)).toBe(1000);
    expect(GatewayClient.backoffMs(20)).toBe(30_000);
  });
});
