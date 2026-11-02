import { test, type Browser } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Scratch probe for the lounge polish round (not committed).
const ON = process.env.SR_E2E_SWEEP === '1';
test.skip(!ON, 'Opt-in: set SR_E2E_SWEEP=1.');
const STATE = 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep/e2e/.state/sweep';
const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8'));
const OUT = 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/lounge';

async function open(browser: Browser, path: string, phone: boolean, dark = false) {
  const ctx = await browser.newContext({
    baseURL: 'http://localhost:5173',
    viewport: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
    colorScheme: dark ? 'dark' : 'light',
    storageState: resolve(STATE, 'owner.json'),
    bypassCSP: true,
  });
  await ctx.addInitScript((t) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }));
  }, dark ? 'lounge-editorial' : 'lounge-light');
  const page = await ctx.newPage();
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') LOG.push('console ' + m.type() + ': ' + m.text().slice(0, 300)); });
  page.on('response', (r) => { if (r.status() >= 400) LOG.push('http ' + r.status() + ' ' + r.url().slice(0, 200)); });
  page.on('requestfailed', (r) => LOG.push('failed ' + r.url().slice(0, 200)));
  await page.goto(path);
  await page.waitForTimeout(Number(process.env.WAIT ?? 5000));
  return page;
}

const LOG: string[] = [];
const PROBE = process.env.PROBE ?? 'file';

test('probe', async ({ browser }) => {
  const phone = process.env.PHONE !== '0';
  const path = (process.env.PROBE_PATH ?? '/channels/{g}/{c}')
    .replace('{g}', world.guildId)
    .replace('{c}', world.channelId)
    .replace('{dm}', world.dmId)
    .replace('{v}', world.voiceChannelId);
  const page = await open(browser, path, phone, process.env.DARK === '1');
  const out = await page.evaluate((sel) => {
    let el: Element | null = document.querySelector(sel);
    const rows: string[] = [];
    while (el && rows.length < 14) {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      rows.push(
        `${el.tagName}.${el.className} w=${Math.round(r.width)} x=${Math.round(r.x)} d=${cs.display} minw=${cs.minWidth} maxw=${cs.maxWidth}`,
      );
      el = el.parentElement;
    }
    return rows.join('\n');
  }, process.env.SEL ?? '.lng-file-name');
  await page.screenshot({ path: `${OUT}-${PROBE}.png` });
  writeFileSync(`${OUT}-${PROBE}.txt`, `${page.url()}\n${out}\n${LOG.join('\n')}\n${process.env.EVAL ? await page.evaluate(process.env.EVAL) : ''}`);
});
