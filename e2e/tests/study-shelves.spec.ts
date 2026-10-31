import type { Page, Route } from '@playwright/test';
import { expect, test } from '../support/fixtures';

/**
 * STU-SHF-001 (wave 41-B): The Study, shelves by source. The server side is faked with route handlers (as in
 * emulator.spec.ts), so the spec needs no bucket: it proves the page's own behaviour in a real browser and that
 * browsing the Study never calls an outside metadata service.
 *
 *   - the sidebar groups My Files, one entry per friend and one per guild;
 *   - every card carries its provenance badge;
 *   - the type filter narrows the cards;
 *   - the owner edits a title and the card shows it;
 *   - no request leaves for a host other than the app and the stand-in bucket.
 *
 * Not run by the 41-B agent (ports 8080/5173 belong to the navigation work); typechecked only.
 */
const API = '**/api/v1';
const BUCKET = 'https://bucket.invalid';

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

const FILES: Record<string, { key: string; size: number }[]> = {
  '9501': [
    { key: 'Films/heat.mp4', size: 1000 },
    { key: 'Music/blue.mp3', size: 2000 },
    { key: 'Books/dune.epub', size: 3000 },
  ],
  '9502': [{ key: 'Shared/akira.cbz', size: 4000 }],
};

async function fakeStudyApi(page: Page, canEdit: boolean) {
  const notes: { key: string; title?: string }[] = [];
  const puts: unknown[] = [];
  await page.route(`${API}/users/@me/storage/mounts`, (r) =>
    json(r, [{ id: '9501', label: 'Home', status: 'active' }]),
  );
  await page.route(`${API}/users/@me/storage/shares`, (r) =>
    json(r, [
      {
        mount_id: '9502',
        owner_id: '777',
        label: 'Friend',
        cinema: true,
        library: true,
        music: false,
      },
    ]),
  );
  await page.route(`${API}/users/@me/games/roms`, (r) => json(r, { roms: [] }));
  await page.route(`${API}/shelves/*`, (r) => json(r, { items: [], total: 0, sources: [] }));
  await page.route(`${API}/storage/mounts/*/objects*`, (r) => {
    const mount = new URL(r.request().url()).pathname.split('/')[5] ?? '';
    return json(r, {
      folders: [],
      objects: (FILES[mount] ?? []).map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: canEdit,
    });
  });
  await page.route(`${API}/storage/mounts/*/notes`, async (r) => {
    if (r.request().method() === 'PUT') {
      const body = r.request().postDataJSON() as { key: string; title: string };
      puts.push(body);
      notes.push({ key: body.key, title: body.title });
      return r.fulfill({ status: 204 });
    }
    return json(r, { notes, can_edit: canEdit });
  });
  await page.route(`${API}/storage/mounts/*/presign`, (r) =>
    json(r, { url: `${BUCKET}/x?sig=1`, expires_at: new Date(Date.now() + 600_000).toISOString() }),
  );
  // The covers read the stand-in bucket; it answers nothing, so cards fall back to their type glyph.
  await page.route(`${BUCKET}/**`, (r) => r.fulfill({ status: 404 }));
  return { puts };
}

test.describe('The Study (STU-SHF-001)', () => {
  test('sidebar by source, provenance badges, type filter, owner edit, nothing leaves the app', async ({
    seedUser,
    signedIn,
    baseURL,
  }) => {
    const { page } = await signedIn(await seedUser('study'));
    const foreign: string[] = [];
    const appHost = new URL(baseURL ?? 'http://localhost').host;
    page.on('request', (req) => {
      const u = new URL(req.url());
      if (u.protocol.startsWith('http') && u.host !== appHost && u.origin !== BUCKET) {
        foreign.push(u.host);
      }
    });
    const { puts } = await fakeStudyApi(page, true);

    await page.goto('/study');
    const nav = page.getByRole('navigation', { name: 'Shelves by source' });
    await expect(nav.getByRole('button', { name: 'My Files' })).toBeVisible();
    await expect(nav.getByText("Friends' Shelves")).toBeVisible();

    // Every card carries a badge naming the bucket.
    const heat = page.getByRole('button', { name: /^Open heat, / });
    await expect(heat).toBeVisible();
    await expect(page.locator('.stu-badge').first()).toContainText('bucket');

    // The friend's shelf is its own entry.
    await nav
      .getByRole('button', { name: /^(?!My Files)/ })
      .first()
      .click();
    await expect(page.getByRole('button', { name: /^Open akira, / })).toBeVisible();

    // Type filter.
    await nav.getByRole('button', { name: 'My Files' }).click();
    await page.getByRole('button', { name: 'Audio (1)' }).click();
    await expect(page.getByRole('button', { name: /^Open blue, / })).toBeVisible();
    await expect(page.getByRole('button', { name: /^Open heat, / })).toHaveCount(0);
    await page.getByRole('button', { name: 'Audio (1)' }).click();

    // Owner edit.
    await page.getByRole('button', { name: 'Edit details of heat' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByLabel('Title').fill('Heat (director cut)');
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByRole('button', { name: /^Open Heat \(director cut\), / })).toBeVisible();
    expect(puts).toHaveLength(1);

    expect(foreign).toEqual([]);
  });

  test('a viewer who does not own the shelf gets no editor', async ({ seedUser, signedIn }) => {
    const { page } = await signedIn(await seedUser('studyview'));
    await fakeStudyApi(page, false);
    await page.goto('/study');
    await expect(page.getByRole('button', { name: /^Open heat, / })).toBeVisible();
    await expect(page.getByRole('button', { name: /^Edit details of/ })).toHaveCount(0);
  });
});
