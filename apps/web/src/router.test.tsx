import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { GatewayClient } from './api/gateway';
import { createAppRouter } from './router';

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
}

describe('routes', () => {
  it.each([
    ['/', 'SIDE ROOM.'],
    ['/login', 'Come on in.'],
    ['/channels', 'Lounge'],
    ['/cinema', 'Cinema'],
    ['/library', 'Library'],
    ['/admin', 'Admin'],
    ['/developers', 'Developers'],
    ['/support', 'Support'],
  ])('renders %s', async (path, heading) => {
    at(path);
    expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
  });

  it('shows only "Log in to accept" on a logged-out invite', async () => {
    at('/invite/abc');
    expect(await screen.findByRole('link', { name: 'Log in to accept' })).toBeInTheDocument();
    expect(screen.getAllByRole('link')).toHaveLength(1);
  });

  it('reads the join code', async () => {
    at('/join/xyz');
    expect(await screen.findByText(/xyz/)).toBeInTheDocument();
  });

  it('has no board route', async () => {
    at('/board');
    await screen.findByText(/page not found/i);
    expect(screen.queryByRole('heading', { name: /board/i })).toBeNull();
  });
});

describe('GatewayClient', () => {
  it('caps backoff', () => {
    expect(GatewayClient.backoffMs(0)).toBe(1000);
    expect(GatewayClient.backoffMs(20)).toBe(30_000);
  });
});
