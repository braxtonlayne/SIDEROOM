import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext } from '@playwright/test';
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
 * XC-VIS-004 (wave 50 css fix): before/after shots and click checks for three layout defects.
 * Local only (SR_P50_CSS=1); shots go to SR_P50_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P50_OUT ?? resolve('test-results/p50-css');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P50_CSS !== '1', 'local visual check only');
test.setTimeout(240_000);

test('select in dialog, settings scroll, party card across themes', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    {
      name: 'Css Booth',
    },
  );
  const channelId = guild.channels.find((c) => c.type === 0)!.id;
  const voice = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
    name: 'Late Room',
    type: 2,
  });
  await apiPatch(o, '/admin/features/moderation.reports', { enabled: true, reason: 'css check' });
  const member = await registerUser(
    (await browser.newContext({ baseURL: ORIGIN })).request,
    'cssmate',
  );
  const mctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(mctx.request, member);
  const invite = await apiPost<{ code: string }>(o, `/channels/${channelId}/invites`, {});
  await apiPost(mctx.request, `/invites/${invite.code}`);
  await apiPost(o, `/channels/${channelId}/messages`, { content: 'a line worth reporting' });
  await apiPost(o, '/watch-parties', {
    channel_id: channelId,
    external: { url: 'https://peertube.test/w/9c9de5e8-0a1e-484a-b099-e80766180a6d' },
  });

  const report: string[] = [];
  for (const theme of THEMES) {
    for (const ctx of [octx, mctx])
      await apiPatch(ctx.request, '/users/@me/settings', { settings: { user_prefs: { theme } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const mk = async (base: BrowserContext) => {
        const state = await base.storageState();
        const c = await browser.newContext({
          baseURL: ORIGIN,
          viewport: size,
          storageState: state,
        });
        await c.addInitScript((t) => {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
          );
        }, theme);
        return c;
      };
      const tag = `${vp}-${theme}`;

      // 3. party strip and card
      let c = await mk(octx);
      let page = await c.newPage();
      await page.goto(`/channels/${guild.id}/${channelId}`);
      await page.waitForTimeout(4500);
      await page.screenshot({ path: resolve(OUT, `party-${tag}.png`) });
      const bg = await page.evaluate(() => {
        const q = (s: string) => {
          const e = document.querySelector(s);
          return e ? [getComputedStyle(e).backgroundColor, getComputedStyle(e).color] : null;
        };
        return { strip: q('.wp-strip'), card: q('.wp-card') };
      });
      report.push(`party ${tag} ${JSON.stringify(bg)}`);
      // 2. settings scroll
      for (const url of [
        `/settings/server/${guild.id}/channels?channel=${voice.id}&tab=voice`,
        '/settings/user/appearance',
      ]) {
        await page.goto(url);
        await page.waitForTimeout(1500);
        await page.mouse.move(size.width / 2, size.height * 0.7);
        await page.mouse.wheel(0, 4000);
        await page.waitForTimeout(400);
        const name = url.includes('channels') ? 'room' : 'user';
        await page.screenshot({ path: resolve(OUT, `settings-${name}-${tag}.png`) });
        const info = await page.evaluate(() => {
          let best = 0;
          document.querySelectorAll('*').forEach((e) => {
            const s = getComputedStyle(e);
            if (/(auto|scroll)/.test(s.overflowY) && e.scrollTop > best) best = e.scrollTop;
          });
          return best;
        });
        report.push(`settings-${name} ${tag} maxScrollTop=${info}`);
      }
      await c.close();

      // 1. select inside a dialog
      c = await mk(mctx);
      page = await c.newPage();
      await page.goto(`/channels/${guild.id}/${channelId}`);
      const msg = page.getByText('a line worth reporting').first();
      await msg.waitFor();
      await msg.click({ button: 'right' });
      await page
        .getByRole('menuitem', { name: /report/i })
        .first()
        .click();
      const dlg = page.getByRole('dialog');
      await dlg.waitFor();
      await dlg.getByRole('combobox').first().click();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `select-open-${tag}.png`) });
      const opt = page.getByRole('option').nth(1);
      const label = ((await opt.textContent()) ?? '').trim();
      try {
        await opt.click({ timeout: 5000 });
        await expect(dlg.getByRole('combobox').first()).toContainText(label.slice(0, 12));
        report.push(`select ${tag} picked "${label}"`);
      } catch (e) {
        report.push(`select ${tag} CLICK FAILED: ${String(e).slice(0, 160)}`);
        if (process.env.SR_P50_BEFORE !== '1') throw e;
      }
      await page.screenshot({ path: resolve(OUT, `select-picked-${tag}.png`) });
      await c.close();
    }
  }
  console.log(report.join('\n'));
  void apiPut;
});
