import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { GatewayClient } from './api/gateway';
import { createAppRouter } from './router';
import { http, HttpResponse, server } from './test/server';

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('routes', () => {
  it.each([
    ['/', /with the door/],
    ['/login', 'Come on in.'],
    ['/login/2fa', 'Your code.'],
    ['/forgot', /fix that/],
    ['/reset/tok', 'Choose a new one.'],
    ['/no-invitation', 'No invitation?'],
    ['/house-notes', 'House notes'],
    ['/channels', 'Friends'],
    ['/cinema', 'Cinema'],
    ['/developers', 'Developers'],
    ['/support', 'Support'],
  ])('renders %s', async (path, heading) => {
    at(path);
    expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
  });

  it('bounces /register to the entrance when no invite code is held', async () => {
    const router = at('/register');
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/'));
  });

  it('shows only "Log in to accept" on a logged-out server invite', async () => {
    server.use(
      http.get('*/api/v1/invites/abc', () =>
        HttpResponse.json({
          code: 'abc',
          guild: { id: '1', name: 'Book Club', icon: null },
          approximate_member_count: 1204,
          approximate_presence_count: 88,
        }),
      ),
    );
    at('/invite/abc');
    expect(await screen.findByRole('heading', { name: 'Book Club' })).toBeInTheDocument();
    expect(screen.getByText(/1,204 members/)).toBeInTheDocument();
    expect(screen.getAllByRole('link')).toHaveLength(1);
    expect(screen.getByRole('link', { name: 'Log in to accept' })).toHaveAttribute(
      'href',
      '/login?next=%2Finvite%2Fabc',
    );
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('shows the generic message for a dead server invite', async () => {
    server.use(
      http.get('*/api/v1/invites/abc', () => HttpResponse.json({ code: 10006 }, { status: 404 })),
    );
    at('/invite/abc');
    expect(await screen.findByText('This invite is invalid or has expired.')).toBeInTheDocument();
    expect(screen.queryAllByRole('link')).toHaveLength(0);
  });

  it('has no board route, link or section', async () => {
    at('/board');
    await screen.findByText(/page not found/i);
    expect(screen.queryByRole('heading', { name: /board/i })).toBeNull();
  });

  it('landing has three worlds and no board', async () => {
    at('/');
    await screen.findByRole('heading', { name: /with the door/ });
    expect(document.body.textContent).not.toMatch(/the board/i);
    expect(
      screen.getByRole('heading', { name: 'One door. A few different worlds.' }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole('heading', { level: 3 })).toHaveLength(3);
  });
});

describe('GatewayClient', () => {
  it('caps backoff', () => {
    expect(GatewayClient.backoffMs(0)).toBe(1000);
    expect(GatewayClient.backoffMs(20)).toBe(30_000);
  });
});
