import type { Page } from '@playwright/test';
import { serveTone } from './playerAudio';

/**
 * A mocked signed-in session for the player's layout checks: every API call answers with an empty success, and
 * the page is a signed-in app route (`/preview-host`), so the gate mounts the host without an API or a database.
 * The Lounge Dark tokens come from the player's own theme root, which is what is being proven.
 */
export async function openMockedApp(page: Page): Promise<void> {
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
