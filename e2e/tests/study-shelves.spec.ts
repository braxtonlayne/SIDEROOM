import { createHash } from 'node:crypto';
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
 *   - the owner edits a title and the card shows it (PATCH of the catalog object, 41-C);
 *   - opening a video plays it from the per-object URL (`GET /shelves/{id}/objects/{sha256(path)}/url`);
 *   - no request leaves for a host other than the app and the stand-in bucket.
 *
 * The real server's side of the same flow (two sessions of one friend account, revoke, delist, no credential in any
 * grantee response) is `shelf_access_holds_over_http` in crates/sr-server/tests/authz_matrix.rs.
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

const sha = (k: string) => createHash('sha256').update(k, 'utf8').digest('hex');

async function fakeStudyApi(page: Page, canEdit: boolean) {
  const notes = new Map<string, { title: string; note: string; cover: string }>();
  const puts: unknown[] = [];
  const minted: string[] = [];
  await page.route(`${API}/shelves/visible`, (r) =>
    json(r, {
      shelves: [
        { shelf_id: '9501', label: 'Home', via: 'own', object_count: 3, can_manage: true },
        {
          shelf_id: '9502',
          label: 'Friend',
          via: 'friend',
          owner_id: '777',
          owner_handle: 'pal',
          object_count: 1,
          can_manage: false,
        },
      ],
    }),
  );
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
  // The catalog, the owner's edit and the per-object URL (41-C). Registered after the broader patterns: the last
  // matching route wins in Playwright.
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) => {
    const shelf = new URL(r.request().url()).pathname.split('/')[4] ?? '';
    return json(r, {
      objects: (FILES[shelf] ?? []).map((f) => ({
        object_hash: sha(f.key),
        path: f.key,
        size: f.size,
        class: 'other',
        title: notes.get(f.key)?.title ?? '',
        cover: notes.get(f.key)?.cover ?? '',
        note: notes.get(f.key)?.note ?? '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: canEdit,
    });
  });
  await page.route(/\/api\/v1\/shelves\/\d+\/objects\/[0-9a-f]{64}(\/url)?$/, (r) => {
    const parts = new URL(r.request().url()).pathname.split('/');
    if (parts[parts.length - 1] === 'url') {
      minted.push(parts[parts.length - 2] ?? '');
      return json(r, {
        url: `${BUCKET}/obj?sig=1`,
        expires_at: new Date(Date.now() + 600_000).toISOString(),
      });
    }
    const hash = parts[parts.length - 1];
    const f = Object.values(FILES)
      .flat()
      .find((x) => sha(x.key) === hash);
    if (!f) return json(r, { code: 10004, message: 'Unknown' }, 404);
    const body = r.request().postDataJSON() as { title: string; note: string; cover: string };
    puts.push({ key: f.key, ...body });
    notes.set(f.key, body);
    return json(r, {});
  });
  // The covers and the player read the stand-in bucket; it answers nothing, so cards fall back to their type glyph.
  await page.route(`${BUCKET}/**`, (r) => r.fulfill({ status: 404 }));
  return { puts, minted };
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
    const { puts, minted } = await fakeStudyApi(page, true);

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

    // Playback: the player's source is the per-object URL minted for sha256 of the file's path.
    await page.getByRole('button', { name: /^Open Heat \(director cut\), / }).click();
    await expect(page.getByRole('dialog').locator('video')).toHaveAttribute(
      'src',
      `${BUCKET}/obj?sig=1`,
    );
    expect(minted).toContain(sha('Films/heat.mp4'));
    await page.keyboard.press('Escape');

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
