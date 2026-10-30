import type { Page } from '@playwright/test';
import { loginApi, type SeededUser } from './fixtures';
import { serveTone } from './playerAudio';

/**
 * A signed-in app page for the player's layout checks, with every API call the page makes answered by an empty
 * success, so the checks need no data. The page is an app route (`/preview-host`).
 *
 * Since D-385 the app document itself is served only to a session: the gate (sr-server, or `vite/publicDev.ts` on
 * the dev server, which asks sr-server) answers an anonymous navigation with `302 /login`, where no player ever
 * mounts. So the context signs in for real through the API first (`user`, a seeded member); only the page's own
 * API calls are mocked. The Lounge Dark tokens come from the player's own theme root, which is what is being proven.
 */
export async function openMockedApp(page: Page, user: SeededUser): Promise<void> {
  await loginApi(page.context().request, user);
  await serveTone(page);
  await page.route('**/api/v1/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: route.request().url().includes('/users/@me') ? '{"id":"1","username":"preview"}' : '{}',
    }),
  );
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/preview-host');
}
