import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../router';
import { warmLoungeRoute } from '../test/warmRoutes';
import { isIndexablePath, useNoindex } from './useNoindex';

warmLoungeRoute();

const robots = () => document.head.querySelectorAll('meta[name="robots"]');

function Probe({ path }: { path: string }) {
  useNoindex(path);
  return null;
}

describe('useNoindex (OPS-IDX-001, D-384)', () => {
  it.each([
    '/login',
    '/register',
    '/invite/x',
    '/help',
    '/u/x',
    '/~x',
    '/cinema',
    '/library',
    '/app',
  ])('adds <meta name="robots" content="noindex"> on %s and removes it on unmount', (path) => {
    const { unmount } = render(<Probe path={path} />);
    const metas = robots();
    expect(metas).toHaveLength(1);
    expect(metas[0]?.getAttribute('content')).toBe('noindex');
    unmount();
    expect(robots()).toHaveLength(0);
  });

  it('adds nothing on the landing page', () => {
    render(<Probe path="/" />);
    expect(robots()).toHaveLength(0);
    expect(isIndexablePath('/')).toBe(true);
    expect(isIndexablePath('/login')).toBe(false);
  });

  it('is applied by the real router: noindex on /login, none on /', async () => {
    const login = render(
      <RouterProvider
        router={createAppRouter(createMemoryHistory({ initialEntries: ['/login'] }))}
      />,
    );
    await vi.waitFor(() => expect(robots()).toHaveLength(1));
    login.unmount();
    expect(robots()).toHaveLength(0);
    render(
      <RouterProvider router={createAppRouter(createMemoryHistory({ initialEntries: ['/'] }))} />,
    );
    await new Promise((r) => setTimeout(r, 50));
    expect(robots()).toHaveLength(0);
  });
});
