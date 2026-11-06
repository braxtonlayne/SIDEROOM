import { expect, type Frame, type Page } from '@playwright/test';
import { test } from '../support/fixtures';
import { builtInActivityBundle } from '../support/activityBundle';
import { startActivityForTwo } from '../support/activities';
import { VOICE_STACK } from '../support/voice';
import { apiPatch } from '../support/fixtures';

/**
 * ACT-SKS-001, the wave 47 gate proof (roadmap P28, D-385, D-389, D-396, D-397, D-460).
 *
 * Sketch & Slander in a voice call: ONE member (the host) and ONE guest on a phone (375 x 812, touch) through the
 * Share link of the Activity panel. The member loads a Guild Prompt Pack, the round runs on that pack, the guest
 * draws or guesses, and then:
 *  - the guest's browser asked the server for the guest-play shell, the one join call and the game's own origin, and
 *    no script it loaded names a chat, member, profile or shelf route;
 *  - the pack reached the game only through the presigned per-object URL: the member's page minted it with
 *    `GET /shelves/{id}/objects/{hash}/url` and fetched it (D-460, `shelfObjectBytes`), the game's frame never
 *    fetched anything, and the guest never touched either.
 *
 * Shelves need a storage mount, which the e2e stack does not set up: the mint route and the bucket are played by
 * routes in the member's context. The bucket answers on the app's own origin (`/e2e-bucket/...`), which is where an
 * operator's same-host storage proxy sits; what is proven is who asked for what, not the storage service itself.
 * D-466 (wave 49): the first pack is chosen with "Choose a file..." in the app's own shelf picker (no second dialog: the
 * pick is the member's yes); the second is pasted as a shelf id and hash, which asks (the fallback).
 *
 * Needs the guest relay, activities and the voice stack: SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1.
 */

const GUEST = process.env.SR_E2E_GUEST === '1';
const ACTIVITIES = process.env.SR_E2E_ACTIVITIES === '1';
const SHELF = '424242';
const HASH = 'e4'.repeat(32);
const HASH2 = '7c'.repeat(32);
const PICKED = {
  name: 'Picked pack',
  prompts: ['sleepy volcano', 'angry toaster', 'polite shark', 'wobbly bridge', 'jazz octopus'],
};
const PACK = {
  name: 'E2E pack',
  prompts: ['purple giraffe', 'melting clock', 'rocket sandwich', 'grumpy cloud', 'dancing cactus'],
};
/** Routes a guest must never reach, and must not find in any script it loaded (D-397). */
const MEMBER_ROUTES =
  /\/api\/v\d+\/(?:channels|users|guilds|shelves|scrapbook|profiles?|messages|relationships|dm)\b/;

/**
 * The visual check (AGENTS.md section 4): with SR_SKETCH_SHOTS=<folder> the run also saves the app's surfaces of this
 * flow (the Activity panel with the game, the Share link dialog and its QR code, the guest page and the game on the
 * phone) in the member's theme SR_SKETCH_THEME (light = Lounge Light, else the default dark theme). Off in CI.
 */
const SHOTS = process.env.SR_SKETCH_SHOTS ?? '';
const THEME = process.env.SR_SKETCH_THEME === 'light' ? 'light' : 'dark';
async function shot(page: Page, name: string) {
  if (!SHOTS) return;
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${SHOTS}/${THEME}-${name}.png` });
}

const game = (page: Page): Frame => {
  const f = page.frames().find((x) => /\.activities\.|\/_activity\//.test(x.url()));
  if (!f) throw new Error('no game frame');
  return f;
};

test.describe('wave 47 gate proof: Sketch & Slander, a member and a guest phone', () => {
  test.skip(
    !GUEST || !ACTIVITIES || !VOICE_STACK,
    'Needs the guest relay, activities and the voice stack: SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1.',
  );

  test('a round on a guild pack; the guest bundle and trace hold no member route; the pack came by URL', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'Chromium resolves *.localhost activity hosts by itself.',
    );
    test.setTimeout(240_000);
    const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
    const session = await startActivityForTwo(
      { browser, browserName, owner, playwright },
      builtInActivityBundle('sketch'),
      'sks',
    );
    try {
      const { a } = session;
      // One member: the developer account that uploaded the bundle leaves the call.
      await session.b.context.close();
      if (SHOTS && THEME === 'light')
        await apiPatch(a.context.request, '/users/@me/settings', {
          settings: { user_prefs: { theme: 'lounge-light' } },
        });

      // The pack: the mint route (the member's own session) and the bucket, played here.
      const minted: string[] = [];
      const bucket: { url: string; fromGame: boolean }[] = [];
      await a.context.route(`**/api/v1/shelves/${SHELF}/objects/*/url`, (route) => {
        minted.push(route.request().url());
        const hash = route.request().url().split('/objects/')[1]?.split('/')[0] ?? '';
        return route.fulfill({
          json: {
            url: `${base}/e2e-bucket/${SHELF}/${hash}?X-Amz-Expires=600&X-Amz-Signature=e2e`,
            expires_at: new Date(Date.now() + 600_000).toISOString(),
          },
        });
      });
      // D-466: what the app's picker lists (the shelves the member can open and one shelf's catalog).
      const entry = (object_hash: string, path: string, size: number) => ({
        object_hash,
        path,
        size,
        class: 'text',
        title: '',
        cover: '',
        note: '',
        updated_at: new Date().toISOString(),
      });
      await a.context.route('**/api/v1/shelves/visible', (route) =>
        route.fulfill({
          json: {
            shelves: [
              {
                shelf_id: SHELF,
                label: 'Home bucket',
                via: 'own',
                object_count: 4,
                can_manage: true,
              },
            ],
          },
        }),
      );
      await a.context.route(new RegExp(`/api/v1/shelves/${SHELF}/objects(\\?.*)?$`), (route) =>
        route.fulfill({
          json: {
            objects: [
              entry(HASH2, 'games/sketch/picked.json', 300),
              entry(HASH, 'games/sketch/e2e-pack.json', 280),
              entry('11'.repeat(32), 'pictures/cat.png', 5000),
              entry('22'.repeat(32), 'games/sketch/huge.json', 900_000),
            ],
            next_cursor: null,
            can_edit: false,
          },
        }),
      );
      await a.context.route('**/e2e-bucket/**', (route) => {
        const req = route.request();
        bucket.push({ url: req.url(), fromGame: req.frame() !== a.page.mainFrame() });
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(req.url().includes(HASH2) ? PICKED : PACK),
        });
      });
      const memberFrameRequests: string[] = [];
      a.page.on('request', (r) => {
        if (r.frame() !== a.page.mainFrame()) memberFrameRequests.push(r.url());
      });

      // The member opens Share link; the QR code is there for the phone.
      await a.page.getByRole('button', { name: 'Share link' }).click();
      await a.page.getByRole('button', { name: 'Create link' }).click();
      const href = await a.page.getByRole('textbox', { name: 'Guest link' }).inputValue();
      expect(href).toMatch(/\/play\/[0-9a-f]{64}#key=[A-Za-z0-9_-]{43}$/);
      await expect(a.page.getByRole('img', { name: 'QR code for the guest link' })).toBeVisible();
      await shot(a.page, 'desktop-share-dialog');
      if (SHOTS) {
        await a.page.setViewportSize({ width: 375, height: 812 });
        await shot(a.page, 'phone-share-dialog');
        await a.page.setViewportSize({ width: 1280, height: 1000 });
      }
      await a.page.getByRole('button', { name: 'Done' }).click();

      // The guest: a phone, in a fresh context (no cookies, no storage).
      const phone = await browser.newContext({
        viewport: { width: 375, height: 812 },
        hasTouch: true,
        isMobile: true,
        colorScheme: THEME,
      });
      const guestRequests: string[] = [];
      const guestScripts: Promise<{ url: string; body: string }>[] = [];
      phone.on('request', (r) => guestRequests.push(r.url()));
      phone.on('response', (r) => {
        const type = r.headers()['content-type'] ?? '';
        if (/javascript|ecmascript/.test(type) || /\.m?js(\?|$)/.test(r.url()))
          guestScripts.push(
            r
              .text()
              .then((body) => ({ url: r.url(), body }))
              .catch(() => ({ url: r.url(), body: '' })),
          );
      });
      const g = await phone.newPage();
      await g.goto(href);
      await shot(g, 'phone-guest-join');
      await g.getByRole('button', { name: 'Join the game' }).click();
      await expect(g.locator('iframe[title^="Game:"]')).toBeVisible({ timeout: 30_000 });
      const gl = g.frameLocator('iframe[title^="Game:"]');
      await expect(gl.locator('#scr-lobby')).toBeVisible({ timeout: 60_000 });

      const fa = game(a.page);
      const fg = game(g);
      await expect
        .poll(
          () =>
            fa.evaluate(
              () =>
                (window as any).__sketch.state()?.players.filter((p: any) => !p.left).length ?? 0,
            ),
          { timeout: 30_000 },
        )
        .toBe(2);
      expect(await fg.evaluate(() => (window as any).__sketch.guest())).toBe(true);
      await shot(g, 'phone-guest-lobby');
      await shot(a.page, 'desktop-panel-lobby');
      if (SHOTS) {
        await a.page.setViewportSize({ width: 375, height: 812 });
        await shot(a.page, 'phone-panel-lobby');
        await a.page.setViewportSize({ width: 1280, height: 1000 });
      }

      // D-466: "Choose a file..." opens the app's own picker over the frame; the pick is the member's yes.
      await expect(fa.locator('#pack-choose')).toBeVisible();
      await fa.locator('#pack-panel').scrollIntoViewIfNeeded();
      await shot(a.page, 'desktop-panel-choose');
      if (SHOTS) {
        await a.page.setViewportSize({ width: 375, height: 812 });
        await fa.locator('#pack-panel').scrollIntoViewIfNeeded();
        await shot(a.page, 'phone-panel-choose');
        await a.page.setViewportSize({ width: 1280, height: 1000 });
      }
      await fa.locator('#pack-choose').click();
      const picker = a.page.getByRole('dialog', { name: 'Choose a file for', exact: false });
      await expect(picker).toBeVisible();
      await expect(picker.getByRole('button', { name: /picked.json/ })).toBeVisible();
      await expect(picker.getByRole('button', { name: /e2e-pack.json/ })).toBeVisible();
      // The picker lists only what the game asked for (.json or .txt, one pack's size): not the picture, not the big file.
      await expect(picker.getByRole('button', { name: /cat.png/ })).toHaveCount(0);
      await expect(picker.getByRole('button', { name: /huge.json/ })).toHaveCount(0);
      await expect(picker.getByText('2 files are hidden', { exact: false })).toBeVisible();
      await shot(a.page, 'desktop-picker');
      if (SHOTS) {
        await a.page.setViewportSize({ width: 375, height: 812 });
        await shot(a.page, 'phone-picker');
        await a.page.setViewportSize({ width: 1280, height: 1000 });
      }
      await picker.getByRole('button', { name: /picked.json/ }).click();
      await expect
        .poll(() => fa.evaluate(() => (window as any).__sketch.state()?.packs ?? []))
        .toEqual([{ name: PICKED.name, count: PICKED.prompts.length }]);
      // No second dialog: the app never asked "Allow" for the object the member picked.
      await expect(a.page.getByRole('button', { name: 'Allow' })).toHaveCount(0);
      expect(minted).toHaveLength(1);
      await fa.locator('#pack-list').scrollIntoViewIfNeeded();
      await shot(a.page, 'desktop-panel-picked');

      // The host loads a second pack by pasting its reference (the fallback): the bridge asks, the member allows,
      // the member's page fetches the URL.
      void fa.evaluate((ref) => (window as any).__sketch.loadPack(ref), `${SHELF}/${HASH}`);
      await a.page.getByRole('button', { name: 'Allow' }).click();
      await expect
        .poll(() => fa.evaluate(() => (window as any).__sketch.state()?.packs ?? []))
        .toEqual([
          { name: PICKED.name, count: PICKED.prompts.length },
          { name: PACK.name, count: PACK.prompts.length },
        ]);
      await fa.evaluate(() =>
        (window as any).__sketch.send({ t: 'settings', rounds: 1, drawSecs: 60, builtIn: false }),
      );
      await fa.evaluate(() => (window as any).__sketch.send({ t: 'start' }));

      // The round: whoever draws picks a prompt from the pack; the other guesses it.
      await expect
        .poll(() => fa.evaluate(() => (window as any).__sketch.state()?.phase))
        .toBe('pick');
      const hostId = await fa.evaluate(() => (window as any).__sketch.self());
      const drawerId = await fa.evaluate(() => (window as any).__sketch.state().drawer);
      const guestDraws = drawerId !== hostId;
      test
        .info()
        .annotations.push({ type: 'drawer', description: guestDraws ? 'guest phone' : 'member' });
      console.log(`[sketch-gate] the drawer is the ${guestDraws ? 'guest phone' : 'member'}`);
      const [fd, fq] = guestDraws ? [fg, fa] : [fa, fg];
      await expect
        .poll(() => fd.evaluate(() => (window as any).__sketch.secret()?.choices.length ?? 0))
        .toBe(3);
      const choices: string[] = await fd.evaluate(() => (window as any).__sketch.secret().choices);
      const prompt = choices[0] ?? '';
      for (const c of choices) expect([...PACK.prompts, ...PICKED.prompts]).toContain(c);
      if (guestDraws) await gl.getByRole('button', { name: prompt }).tap();
      else await fa.evaluate(() => (window as any).__sketch.send({ t: 'pick', i: 0 }));
      await expect
        .poll(() => fd.evaluate(() => (window as any).__sketch.secret()?.prompt ?? ''))
        .toBe(prompt);

      if (guestDraws) {
        // A finger on the phone's pad: pointer events through the touch screen.
        const pad = gl.locator('#pad');
        await expect(gl.locator('#tools')).toBeVisible();
        const box = (await pad.boundingBox())!;
        await g.mouse.move(box.x + 40, box.y + 40);
        await g.mouse.down();
        for (let i = 1; i <= 10; i++) await g.mouse.move(box.x + 40 + i * 25, box.y + 40 + i * 15);
        await g.mouse.up();
      } else {
        await fa.evaluate(() => (window as any).__sketch.line(100, 100, 800, 600));
      }
      await expect
        .poll(() => fq.evaluate(() => (window as any).__sketch.lastState()?.turnNo ?? 0))
        .toBeGreaterThan(0);
      // The guesser's page holds the ink (the pad drew it), and the prompt only as a mask.
      const mask: string = await fq.evaluate(() => (window as any).__sketch.state().mask);
      await shot(a.page, 'desktop-panel-draw');
      await shot(g, 'phone-guest-draw');
      expect(mask).not.toContain(prompt);

      if (guestDraws) {
        await fa.evaluate((p) => (window as any).__sketch.send({ t: 'guess', text: p }), prompt);
      } else {
        await gl.locator('#guess').fill(prompt.toUpperCase());
        await gl.locator('#guess').press('Enter');
      }
      await expect
        .poll(() => fa.evaluate(() => (window as any).__sketch.state()?.phase), { timeout: 15_000 })
        .toBe('reveal');
      const after = await fg.evaluate(() => (window as any).__sketch.state());
      expect(after.word).toBe(prompt);
      expect(after.players.every((p: any) => p.score > 0)).toBe(true);
      await expect(gl.locator('#overlay')).toContainText(prompt);
      await shot(a.page, 'desktop-panel-reveal');
      await shot(g, 'phone-guest-reveal');

      // ---- The gate ------------------------------------------------------------------------------------------
      // The guest's network trace: the shell, one join call, the game's own origin; no member route, no shelf, no
      // bucket.
      const appHost = `${session.appId}.activities.`;
      for (const u of guestRequests) {
        const url = new URL(u);
        if (url.hostname.includes('.activities.'))
          expect(url.hostname, u).toMatch(new RegExp(`^${appHost}`));
        if (url.pathname.startsWith('/api/'))
          expect(url.pathname, u).toMatch(/^\/api\/v1\/play\/[0-9a-f]{64}\/join$/);
        expect(u, u).not.toMatch(MEMBER_ROUTES);
        expect(u, u).not.toMatch(/e2e-bucket|\/shelves\//);
      }
      expect(guestRequests.some((u) => /\/api\/v1\/play\/[0-9a-f]{64}\/join$/.test(u))).toBe(true);
      // The guest's bundle: every script it loaded (the guest shell and the game) names no member route.
      const scripts = await Promise.all(guestScripts);
      expect(scripts.length).toBeGreaterThan(3);
      for (const s of scripts) expect(MEMBER_ROUTES.test(s.body), s.url).toBe(false);
      expect(await phone.cookies()).toEqual([]);

      // The pack: one mint through the per-object route, one fetch of the presigned URL from the member's page,
      // nothing from the game's frame.
      expect(minted).toHaveLength(2);
      expect(bucket).toHaveLength(2);
      for (const b of bucket) {
        expect(b.url).toContain('X-Amz-Signature=');
        expect(b.fromGame).toBe(false);
      }
      expect(memberFrameRequests.filter((u) => /e2e-bucket|\/shelves\//.test(u))).toEqual([]);
      await phone.close();
    } finally {
      await session.close();
    }
  });
});
