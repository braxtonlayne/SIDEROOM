import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../router';
import { http, HttpResponse, server } from '../test/server';
import { resetPrefs, signedIn } from '../settings/user/__tests__/helpers';
import { warmLoungeRoute } from '../test/warmRoutes';

warmLoungeRoute();

/**
 * XC-A11Y-004: axe over the signed-in routes, one table row per route. The public routes run in a real
 * browser (e2e/static/a11y.spec.ts); these run in jsdom with the API mocked, so they catch roles, names and
 * ARIA validity but not contrast or landmark layout (docs/dev/a11y-checklist.md).
 */
interface Row {
  path: string;
  /** Text (heading or label) that proves the route finished its first render. */
  ready: string | RegExp;
  /** Optional text that appears once the route's data has loaded. */
  until?: string;
  handlers?: () => void;
}

const guild = { id: '5', name: 'Book Club' };

const ROUTES: Row[] = [
  { path: '/channels/@me', ready: 'Friends' },
  { path: '/settings/user/account', ready: 'My Account', handlers: settingsApi },
  { path: '/settings/user/notifications', ready: 'Notifications', handlers: settingsApi },
  { path: '/settings/user/appearance', ready: 'Appearance', handlers: settingsApi },
  { path: '/settings/user/privacy', ready: /Privacy/, handlers: settingsApi },
  { path: '/settings/user/accessibility', ready: 'Accessibility', handlers: settingsApi },
  {
    path: '/share?text=Hello&url=https%3A%2F%2Fexample.com',
    ready: 'Share to SIDE ROOM',
    until: '#general',
    handlers: () =>
      server.use(
        http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([guild])),
        http.get('*/api/v1/guilds/5/channels', () =>
          HttpResponse.json([{ id: '50', type: 0, name: 'general', guild_id: '5', position: 0 }]),
        ),
        http.get('*/api/v1/users/@me/channels', () => HttpResponse.json([])),
      ),
  },
];

function settingsApi() {
  server.use(
    http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([guild])),
    http.get('*/api/v1/users/@me/profile', () =>
      HttpResponse.json({
        user: { id: '1', username: 'alice', global_name: 'Alice', avatar: null },
        user_profile: { bio: '', pronouns: '', banner: null, accent_color: null },
        connected_accounts: [],
      }),
    ),
  );
}

describe('axe on signed-in routes (XC-A11Y-004)', () => {
  beforeEach(() => {
    resetPrefs();
    signedIn();
  });

  it.each(ROUTES)('$path has no violations', async ({ path, ready, until, handlers }) => {
    handlers?.();
    const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
    const { container } = render(<RouterProvider router={router} />);
    await screen.findByRole('heading', { name: ready });
    if (until) await screen.findByText(until);
    // Let the first round of requests settle so lists and cards are in the tree.
    await waitFor(() => expect(container.querySelector('[aria-busy="true"]')).toBeNull());
    expect(await axeViolations(container)).toEqual([]);
  });
});
