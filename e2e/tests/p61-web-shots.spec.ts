import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Route } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  apiPut,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-135 / 137 (wave 61): a failed inline edit keeps the editor open with the text; View Server As Role
 * with a role the engine refuses; a group DM made while adding some members failed. Editorial, Lounge Light and
 * Lounge Dark, desktop and phone. Local only (SR_P61_WEB=1); shots go to SR_P61_WEB_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P61_WEB_OUT ?? resolve('test-results/p61-web');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const GUILD = 'Edit Failure Booth';

test.skip(process.env.SR_P61_WEB !== '1', 'local visual check only');
test.setTimeout(1_200_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const boom = (r: Route) =>
  r.fulfill({ status: 500, contentType: 'application/json', body: '{"message":"boom"}' });

test('failed edits, a refused role and a half-made group say so', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: GUILD },
  );
  const channelId = guild.channels.filter((c) => c.type === 0)[0]!.id;

  // Three friends for the group DM.
  const friends: { id: string; username: string }[] = [];
  for (const label of ['amy', 'ben', 'cleo']) {
    const rctx = await browser.newContext({ baseURL: ORIGIN });
    const u = await registerUser(rctx.request, label);
    await rctx.close();
    const uctx = await browser.newContext({ baseURL: ORIGIN });
    await loginApi(uctx.request, u);
    await apiPut(o, `/users/@me/relationships/${u.id}`, { type: 1 });
    await apiPut(uctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
    await uctx.close();
    friends.push(u);
  }
  const [amy, ben, cleo] = friends as [
    (typeof friends)[number],
    (typeof friends)[number],
    (typeof friends)[number],
  ];

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      // 1. An inline edit whose save fails stays open with the typed text and an error line; then it saves.
      const m = await apiPost<{ id: string }>(o, `/channels/${channelId}/messages`, {
        content: `first words ${tag}`,
      });
      await page.route(new RegExp(`/api/v1/channels/${channelId}/messages/${m.id}$`), (r) =>
        r.request().method() === 'PATCH' ? boom(r) : r.fallback(),
      );
      await page.goto(`/channels/${guild.id}/${channelId}`);
      const box = page.getByRole('textbox', { name: /^Message #/ });
      await box.click();
      await page.keyboard.press('ArrowUp');
      const editor = page.getByRole('textbox', { name: 'Edit message' });
      await expect(editor).toBeVisible();
      await editor.fill(`second words ${tag}, a longer line so the text is plainly kept`);
      await page.keyboard.press('Enter');
      await expect(
        page.locator('.lng-inline-edit').getByText('Your edit was not saved. Try again.'),
      ).toBeVisible();
      await expect(editor).toHaveValue(/second words/);
      await shot('edit-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await page.keyboard.press('Enter');
      await expect(editor).toHaveCount(0);
      await expect(page.getByText(`second words ${tag}`)).toBeVisible();
      await shot('edit-saved');

      // 2. View Server As Role: a role the permission engine refuses is an error with Try again.
      await page.route(new RegExp(`/api/v1/guilds/${guild.id}/roles$`), async (r) => {
        const res = await r.fetch();
        const roles = (await res.json()) as { id: string; permissions: string }[];
        await r.fulfill({
          response: res,
          json: roles.map((x) => (x.id === guild.id ? { ...x, permissions: 'not-a-number' } : x)),
        });
      });
      await page.goto(`/settings/server/${guild.id}/roles`);
      await page.getByRole('button', { name: 'View Server As Role' }).click();
      const dialog = page.getByRole('dialog', { name: 'View Server As Role' });
      await expect(dialog.getByText(/could not be worked out/)).toBeVisible();
      await expect(dialog.getByText('Working it out')).toHaveCount(0);
      await shot('viewas-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await dialog.getByRole('button', { name: 'Try again' }).click();
      await expect(dialog.getByRole('list', { name: 'Visible channels' })).toBeVisible();
      await shot('viewas-retried');

      // 3. Adding people to a 1:1 DM makes a group; the second person cannot be added: the group opens and
      //    the dialog names who was not added.
      const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', {
        recipient_id: amy.id,
      });
      await page.route(new RegExp(`/api/v1/channels/[^/]+/recipients/${cleo.id}$`), (r) =>
        r.request().method() === 'PUT' ? boom(r) : r.fallback(),
      );
      await page.goto(`/channels/@me/${dm.id}`);
      await page.getByRole('button', { name: 'Add Friends to DM' }).click();
      const add = page.getByRole('dialog', { name: 'Add Friends to DM' });
      await add.getByRole('checkbox', { name: new RegExp(ben.username, 'i') }).check();
      await add.getByRole('checkbox', { name: new RegExp(cleo.username, 'i') }).check();
      await shot('adddm-picked');
      await add.getByRole('button', { name: 'Add to DM' }).click();
      const partial = page.getByRole('dialog', { name: 'Some people were not added' });
      await expect(partial.getByText(new RegExp(`${cleo.username} could not be`))).toBeVisible();
      await shot('adddm-partial');
      await partial.getByRole('button', { name: /Done/ }).click();
      await expect(page).not.toHaveURL(new RegExp(`/channels/@me/${dm.id}$`));
      await shot('adddm-group-open');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
