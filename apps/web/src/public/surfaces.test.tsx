import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { nav } from '../lib/enterApp';
import { createPublicRouter } from '../test/publicRouter';
import { http, HttpResponse, server } from '../test/server';

function at(path: string) {
  const router = createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  vi.spyOn(nav, 'assign').mockImplementation(() => {});
  vi.spyOn(nav, 'replace').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('the landing surface', () => {
  it.each([['/'], ['/join/abc']])('renders %s', async (path) => {
    at(path);
    expect(await screen.findByRole('heading', { name: /with the door/ })).toBeInTheDocument();
  });

  it('names no section of the app and has no teaser strip (owner O-7)', async () => {
    at('/');
    await screen.findByRole('heading', { name: /with the door/ });
    const text = document.body.textContent ?? '';
    expect(text).not.toMatch(/lounge|cinema|library|a few different worlds|a glimpse/i);
    expect(screen.queryAllByRole('heading', { level: 3 })).toHaveLength(0);
  });

  it('shows "nothing here" for a path that is not its own, never a redirect', async () => {
    const router = createPublicRouter('/', createMemoryHistory({ initialEntries: ['/board'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole('heading', { name: 'Nothing here' })).toBeInTheDocument();
    expect(nav.assign).not.toHaveBeenCalled();
  });

  it('hands a signed-in member to /app with a document load', async () => {
    server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1' })));
    at('/');
    await waitFor(() => expect(nav.assign).toHaveBeenCalledWith('/app'));
  });
});

describe('the auth surface', () => {
  it.each([
    ['/login', 'Come on in.'],
    ['/login/2fa', 'Your code.'],
    ['/forgot', /fix that/],
    ['/reset/tok', 'Choose a new one.'],
  ])('renders %s', async (path, heading) => {
    at(path);
    expect(await screen.findByRole('heading', { name: heading })).toBeInTheDocument();
  });

  it('bounces /register to the landing page when no invite code is held', async () => {
    at('/register');
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith('/'));
  });
});

describe('the invite surface', () => {
  it('renders /no-invitation', async () => {
    at('/no-invitation');
    expect(await screen.findByRole('heading', { name: 'No invitation?' })).toBeInTheDocument();
  });

  it('shows a visitor only "Log in to accept", with nothing about the server', async () => {
    server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({}, { status: 401 })));
    at('/invite/abc');
    expect(await screen.findByRole('link', { name: 'Log in to accept' })).toHaveAttribute(
      'href',
      '/login?next=%2Finvite%2Fabc',
    );
    expect(screen.queryByRole('button')).toBeNull();
    expect(nav.assign).not.toHaveBeenCalled();
  });

  it('hands a signed-in member to the invite page inside the app', async () => {
    server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1' })));
    at('/invite/abc');
    await waitFor(() => expect(nav.assign).toHaveBeenCalledWith('/app?goto=%2Finvite%2Fabc'));
  });

  it('sends a visitor of a template to sign-in', async () => {
    server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({}, { status: 401 })));
    at('/template/t1');
    expect(await screen.findByRole('link', { name: 'Log in' })).toHaveAttribute(
      'href',
      '/login?next=%2Ftemplate%2Ft1',
    );
  });
});
