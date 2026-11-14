import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
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
 * SEC-AUD-246..248 (wave 70; D-729, D-730): a message's text outside the message (the reply line, the Inbox, the
 * DM toast, the delete confirmation) is plain text; a blocked author's is not shown (reply line, Inbox, pins,
 * search, link card, typing line). Real stack, three accounts. Editorial, Lounge Light and Lounge Dark, desktop
 * and phone. Local only (SR_P70_LINES=1); shots go to SR_P70_LINES_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P70_LINES_OUT ?? resolve('test-results/p70-lines');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P70_LINES !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
  extra: Record<string, unknown> = {},
) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100, ...extra },
  );
  return c;
}

async function member(browser: Browser, label: string, inviteCode: string) {
  const ctx = await browser.newContext({ baseURL: ORIGIN });
  const user = await registerUser(ctx.request, label);
  await loginApi(ctx.request, user);
  await apiPost(ctx.request, `/invites/${inviteCode}`, {});
  return { ctx, user };
}

test('the lines around the list: plain text, and nothing of a blocked author', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Lines Booth' },
  );
  const g = guild.id;
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const channel2 = (
    await apiPost<{ id: string }>(o, `/guilds/${g}/channels`, { name: 'elsewhere', type: 0 })
  ).id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const bob = await member(browser, 'bobby', invite.code);
  const carol = await member(browser, 'carolyn', invite.code);

  // Bob and the Owner are friends (the DM toast), Bob writes the message with a spoiler and a mention.
  await apiPut(o, `/users/@me/relationships/${bob.user.id}`, { type: 1 });
  await apiPut(bob.ctx.request, `/users/@me/relationships/${me.id}`, { type: 1 });
  const dm = await apiPost<{ id: string }>(bob.ctx.request, '/users/@me/channels', {
    recipient_id: me.id,
  });
  const secret = `psst ||the ending|| <@${me.id}>`;
  const bobMsg = await apiPost<{ id: string }>(bob.ctx.request, `/channels/${channel}/messages`, {
    content: secret,
  });
  await apiPost(o, `/channels/${channel}/messages`, {
    content: 'what do you mean? tell me more',
    message_reference: { message_id: bobMsg.id },
  });
  await apiPut(o, `/channels/${channel}/pins/${bobMsg.id}`, {});
  await apiPost(carol.ctx.request, `/channels/${channel}/messages`, {
    content: `see ${ORIGIN}/channels/${g}/${channel}/${bobMsg.id}`,
  });
  await apiPost(bob.ctx.request, `/channels/${channel}/messages`, {
    content: 'a spoiler-free needlehaystack line from bobby',
  });

  const theme = (t: string, extra: Record<string, unknown> = {}) =>
    apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100, ...extra } },
    });
  const open = async (t: string, size: { width: number; height: number }, extra = {}) => {
    await theme(t, extra);
    const c = await context(browser, octx, size, t, extra);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(`/channels/${g}/${channel2}`);
    await expect(page.getByRole('main')).toBeVisible();
    await page.waitForTimeout(500);
    return { c, page };
  };
  const goChannel = async (page: Page) => {
    await page.goto(`/channels/${g}/${channel}`);
    await expect(page.getByRole('main')).toBeVisible();
    await page.waitForTimeout(700);
  };
  const shot = async (target: Locator | Page, name: string) => {
    await target.screenshot({ path: resolve(OUT, `${name}.png`) });
  };
  const replyRow = (page: Page) =>
    page.locator('.lng-msg').filter({ has: page.locator('.lng-reply') });

  const scene = async (
    phase: 'before' | 'after',
    vp: keyof typeof VIEWPORTS,
    t: (typeof THEMES)[number],
  ) => {
    const tag = `${phase}-${vp}-${t}`;
    const { c, page } = await open(t, VIEWPORTS[vp]);
    await goChannel(page);

    // The reply line (normal density).
    const row = replyRow(page).first();
    await expect(row).toBeVisible();
    const lineText = (await row.locator('.lng-reply').innerText()).replace(/\s+/g, ' ');
    console.log(`RECORDED reply line ${tag}:`, lineText);
    if (phase === 'before') {
      expect(lineText).toContain('psst [spoiler]');
      expect(lineText).not.toContain('the ending');
      expect(lineText).not.toContain(me.id);
    } else {
      expect(lineText).toContain('blocked message');
      expect(lineText).not.toContain('psst');
    }
    await shot(row, `reply-${tag}`);

    // The Inbox's mention list.
    await page.getByRole('button', { name: 'Inbox' }).first().click();
    const inbox = page.locator('.lng-inbox');
    await expect(inbox).toBeVisible();
    await page.waitForTimeout(900);
    const inboxText = (await inbox.innerText()).replace(/\s+/g, ' ');
    console.log(`RECORDED inbox ${tag}:`, inboxText.slice(0, 200));
    expect(inboxText).not.toContain('the ending');
    expect(inboxText).not.toContain(me.id);
    if (phase === 'after') expect(inboxText).not.toContain('bobby');
    else expect(inboxText).toContain('psst [spoiler]');
    await shot(page, `inbox-${tag}`);
    await page.keyboard.press('Escape');
    await page.mouse.click(5, 5);

    // The pins panel.
    await page.getByRole('button', { name: 'Pinned Messages' }).first().click();
    const pins = page.locator('.lng-pins-pop');
    await expect(pins).toBeVisible();
    await page.waitForTimeout(700);
    const pinText = (await pins.innerText()).replace(/\s+/g, ' ');
    console.log(`RECORDED pins ${tag}:`, pinText.slice(0, 200));
    if (phase === 'after') expect(pinText).not.toContain('psst');
    await shot(page, `pins-${tag}`);
    if (phase === 'after') {
      await pins.getByRole('button', { name: /^Show message/ }).click();
      await page.waitForTimeout(200);
      expect(await pins.innerText()).toContain('psst');
      await shot(page, `pins-revealed-${tag}`);
    }
    await page.keyboard.press('Escape');
    await page.mouse.click(5, 5);

    // The message-link card.
    const card = page.locator('.lng-msglink').first();
    await expect(card).toBeVisible();
    const cardText = (await card.innerText()).replace(/\s+/g, ' ');
    console.log(`RECORDED link card ${tag}:`, cardText);
    if (phase === 'after') {
      expect(cardText).toContain('blocked message');
      expect(cardText).not.toContain('psst');
    }
    await shot(card, `linkcard-${tag}`);

    // Search results: bobby's line is a hit, with the lines either side of it.
    const searchBox = page.getByRole('searchbox').first();
    if (vp === 'phone') {
      await page
        .getByRole('button', { name: /search/i })
        .first()
        .click()
        .catch(() => undefined);
    }
    await searchBox.fill('needlehaystack');
    await searchBox.press('Enter');
    const panel = page.getByRole('region', { name: 'Search results' });
    await expect(panel).toBeVisible();
    await page.waitForTimeout(1500);
    const searchText = (await panel.innerText()).replace(/\s+/g, ' ');
    console.log(`RECORDED search ${tag}:`, searchText.slice(0, 240));
    if (phase === 'after') {
      expect(searchText).not.toContain('needlehaystack');
      expect(searchText).toContain('blocked message');
    }
    await shot(page, `search-${tag}`);
    await page.keyboard.press('Escape');

    await c.close();
  };

  // The reply line in compact density, the delete confirmation and the DM toast: before the block.
  const extras = async (vp: keyof typeof VIEWPORTS, t: (typeof THEMES)[number]) => {
    const tag = `${vp}-${t}`;
    {
      const { c, page } = await open(t, VIEWPORTS[vp], { messageDisplay: 'compact' });
      await goChannel(page);
      const row = replyRow(page).first();
      await expect(row).toBeVisible();
      await shot(row, `reply-compact-before-${tag}`);
      await c.close();
    }
    {
      const { c, page } = await open(t, VIEWPORTS[vp]);
      await goChannel(page);
      const target = page.locator('.lng-msg').filter({ hasText: 'psst' }).first();
      await target.click({ button: 'right' });
      await page.getByRole('menuitem', { name: /Delete/ }).click();
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      const dText = (await dialog.innerText()).replace(/\s+/g, ' ');
      console.log(`RECORDED delete dialog ${tag}:`, dText);
      expect(dText).toContain('psst [spoiler]');
      expect(dText).not.toContain('the ending');
      await page.waitForTimeout(300);
      await shot(page, `delete-${tag}`);
      await c.close();
    }
    {
      // The Owner is on another channel, the window focused; Bob writes a DM with a spoiler.
      const { c, page } = await open(t, VIEWPORTS[vp]);
      await page.bringToFront();
      await apiPost(bob.ctx.request, `/channels/${dm.id}/messages`, {
        content: `dm ||the plot twist|| for <@${me.id}> ${tag}`,
      });
      const toast = page.getByText(/sent you a message/).first();
      await expect(toast).toBeVisible({ timeout: 15_000 });
      await page.waitForTimeout(500);
      const body = (await page.locator('body').innerText()).replace(/\s+/g, ' ');
      console.log(`RECORDED toast ${tag}:`, body.match(/sent you a message.{0,80}/)?.[0]);
      expect(body).not.toContain('the plot twist');
      expect(body).not.toContain(`<@${me.id}>`);
      await shot(page, `toast-${tag}`);
      await c.close();
    }
  };

  for (const t of THEMES) {
    for (const vp of ['desktop', 'phone'] as const) {
      await scene('before', vp, t);
      await extras(vp, t);
    }
  }

  // The typing line: bobby is typing in the channel the Owner has open, before and after the block.
  {
    const { c, page } = await open('lounge-light', VIEWPORTS.desktop);
    await goChannel(page);
    await apiPost(bob.ctx.request, `/channels/${channel}/typing`, {});
    const typing = page.locator('.lng-typing');
    await expect(typing).toContainText(/typing/, { timeout: 8_000 });
    await shot(page, 'typing-before-desktop-lounge-light');
    await apiPut(o, `/users/@me/relationships/${bob.user.id}`, { type: 2 });
    await expect(typing).not.toContainText(/typing/, { timeout: 12_000 });
    await apiPost(bob.ctx.request, `/channels/${channel}/typing`, {});
    await page.waitForTimeout(1500);
    expect(await typing.innerText()).not.toMatch(/typing/);
    await shot(page, 'typing-after-desktop-lounge-light');
    await c.close();
  }
  // Still blocked. A phone shot of the line after the block (and bobby typing again).
  {
    const { c, page } = await open('lounge-light', VIEWPORTS.phone);
    await goChannel(page);
    await apiPost(bob.ctx.request, `/channels/${channel}/typing`, {});
    await page.waitForTimeout(1500);
    expect(await page.locator('.lng-typing').innerText()).not.toMatch(/typing/);
    await shot(page, 'typing-after-phone-lounge-light');
    await c.close();
  }

  for (const t of THEMES) {
    for (const vp of ['desktop', 'phone'] as const) {
      await scene('after', vp, t);
    }
  }
  await octx.close();
  await bob.ctx.close();
  await carol.ctx.close();
});
