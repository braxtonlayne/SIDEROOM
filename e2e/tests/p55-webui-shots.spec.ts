import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-046 / 047 (wave 55, D-577, D-578): the failure states that are now inline instead of a toast (channel
 * list, member list, Server Settings, soundboard favourites sync), the danger text token (kit error text, the
 * refused invite on the landing) and the E2EE shield's "new privacy code" notice; Editorial, Lounge Light and
 * Lounge Dark, desktop and phone. Local only (SR_P55_WEBUI=1); shots go to SR_P55_WEBUI_OUT.
 *
 * The gateway sends the server's channels and members in GUILD_CREATE, so the page's gateway socket is routed
 * and that one dispatch is dropped: the app then loads the lists over REST, which the page is told failed.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P55_WEBUI_OUT ?? resolve('test-results/p55-webui');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const GUILD = 'Quiet Failure Booth';

test.skip(process.env.SR_P55_WEBUI !== '1', 'local visual check only');
test.setTimeout(1_500_000);

async function context(
  browser: Browser,
  base: BrowserContext | null,
  size: { width: number; height: number },
  theme: string | null,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    ...(base ? { storageState: await base.storageState() } : {}),
  });
  if (theme)
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
const failOn = (page: Page, glob: string | RegExp, method = 'GET') =>
  page.route(glob, (r) => (r.request().method() === method ? boom(r) : r.fallback()));

const notes: string[] = [];
/** A check that, failing, is noted (and printed at the end) instead of stopping the remaining shots. */
const soft = (label: string, check: Promise<unknown>) =>
  check.catch((e: unknown) => notes.push(`${label}: ${String(e).slice(0, 200)}`));

/** Forward the gateway both ways, but drop the GUILD_CREATE of one server. */
async function dropGuildCreate(page: Page, guildId: string, tag: string) {
  const seen: string[] = [];
  await page.routeWebSocket(/gateway/, (ws) => {
    const server = ws.connectToServer();
    server.onMessage((m) => {
      const text = typeof m === 'string' ? m : m.toString('utf8');
      const t = /"t"\s*:\s*"([A-Z_]+)"/.exec(text)?.[1] ?? 'op';
      if (seen.length < 30) seen.push(t);
      if (t === 'GUILD_CREATE' && text.includes(guildId)) {
        notes.push(`${tag}: dropped GUILD_CREATE`);
        return;
      }
      ws.send(m);
    });
  });
  return seen;
}

test('failure states are inline, danger text and the new privacy code notice', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: GUILD },
  );
  const text = guild.channels.find((c) => c.type === 0)!;
  const voice = guild.channels.find((c) => c.type === 2) ?? text;
  const lists = new RegExp(`/api/v1/guilds/${guild.id}(/channels|/roles|/members)?(\\?|$)`);

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      // 1. Channel list (and on desktop the member list) that failed to load: inline rows, no toast.
      const seen = await dropGuildCreate(page, guild.id, tag);
      const asked: string[] = [];
      page.on('request', (r) => {
        if (lists.test(r.url())) asked.push(r.url().replace(ORIGIN, ''));
      });
      await failOn(page, lists);
      await page.goto(
        vp === 'phone' ? `/channels/${guild.id}` : `/channels/${guild.id}/${text.id}`,
      );
      await soft(
        `${tag} channels row`,
        expect(page.getByText('The channels could not be loaded.').first()).toBeVisible(),
      );
      if (vp === 'desktop')
        await soft(
          `${tag} members row`,
          expect(page.getByText('The member list could not be loaded.')).toBeVisible(),
        );
      notes.push(`${tag}: frames ${seen.join(',')}; list requests ${asked.join(' ')}`);
      await shot('lists-failed');

      // 2. Server Settings while the server cannot be loaded: says so instead of loading forever.
      await page.goto(`/settings/server/${guild.id}`);
      await soft(
        `${tag} server settings row`,
        expect(page.getByText('This server could not be loaded.')).toBeVisible({ timeout: 20_000 }),
      );
      await shot('server-settings-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();

      // 3. The kit's error text (Server Settings > Channels, an emptied channel name).
      const c2 = await context(browser, octx, size, theme);
      const p2 = await c2.newPage();
      p2.setDefaultTimeout(20_000);
      await p2.goto(`/settings/server/${guild.id}/channels?channel=${text.id}`);
      await soft(`${tag} kit error`, p2.getByLabel('Channel name').first().fill(''));
      await soft(
        `${tag} kit error`,
        expect(p2.getByText('Name must be between 1 and 100 characters.')).toBeVisible(),
      );
      await p2.waitForTimeout(500);
      await p2.screenshot({ path: resolve(OUT, `kit-input-error-${tag}.png`) });

      // 4. A call: the soundboard favourites sync failed (one line, no toast), then the new privacy code notice.
      await p2.goto(`/channels/@me`);
      await p2.waitForTimeout(1500);
      await p2.evaluate(`(async () => {
        const me = await (await fetch('/api/v1/users/@me', { credentials: 'include' })).json();
        const st = await import('/src/store/index.ts');
        st.useStore.setState((s) => ({ users: { ...s.users, '9990001': { id: '9990001', username: 'robin', global_name: 'Robin' } } }));
        const v = await import('/src/voice/session.ts');
        v.useVoiceSession.setState({ status: 'connected', guildId: '${guild.id}', channelId: '${voice.id}', channelName: 'General' });
        const m = await import('/src/voice/e2ee/store.ts');
        m.setE2ee({
          active: true,
          viewer: me.id,
          selfKey: 'aa',
          keyChanged: ['9990001'],
          pairwise: { '9990001': '31415 92653 58979 32384 62643 38327' },
          call: {
            status: 'secure',
            epoch: 4,
            callCode: '27182 81828 45904 52353 60287 47135',
            members: [
              { userId: me.id, identityKey: 'aa' },
              { userId: '9990001', identityKey: 'bb' },
            ],
            error: null,
          },
        });
      })()`);
      await p2.waitForTimeout(500);
      await failOn(p2, /\/api\/v1\/users\/@me\/settings$/, 'PATCH');
      await soft(
        `${tag} soundboard`,
        p2.getByRole('button', { name: 'Soundboard' }).first().click(),
      );
      await soft(
        `${tag} favorite`,
        p2
          .getByRole('button', { name: /^Favorite / })
          .first()
          .click(),
      );
      await soft(`${tag} unsynced`, expect(p2.getByTestId('soundboard-unsynced')).toBeVisible());
      await p2.waitForTimeout(500);
      await p2.screenshot({ path: resolve(OUT, `soundboard-unsynced-${tag}.png`) });
      await p2.keyboard.press('Escape');
      await p2.unrouteAll({ behavior: 'ignoreErrors' });

      await soft(
        `${tag} shield`,
        p2
          .getByRole('button', { name: /new privacy code/ })
          .first()
          .click(),
      );
      await soft(
        `${tag} shield notice`,
        expect(p2.getByText(/has a new privacy code/)).toBeVisible(),
      );
      await p2.waitForTimeout(500);
      await p2.screenshot({ path: resolve(OUT, `shield-new-code-${tag}.png`) });
      await c2.close();
    }
  }

  // 5. The refused invite on the landing (signed out; the entrance has one look, no theme choice).
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    const c = await context(browser, null, size, null);
    const page = await c.newPage();
    await page.goto('/');
    const input = page.locator('#invite-code');
    await input.fill('ZZZZZZZZZZ');
    await input.press('Enter');
    await soft(
      `${vp} invite`,
      expect(page.locator('#invite-msg[data-tone="bad"]')).not.toBeEmpty(),
    );
    await page.waitForTimeout(800);
    await page.screenshot({ path: resolve(OUT, `entrance-invite-refused-${vp}.png`) });
    await c.close();
  }

  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  console.log(['p55-webui notes:', ...notes].join('\n'));
});
