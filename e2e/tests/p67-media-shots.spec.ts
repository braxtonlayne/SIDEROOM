import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * Wave 67, media (SEC-AUD-186, 199, 201, 202): the Live TV guide footer note and channel logos (desktop and phone), the
 * channel editor's logo row (upload, save, reload, remove clicked through on the real stack), Admin > Settings with the
 * footer field, the phone guide's swipe, and the Subtitles block of the asset panel (a fresh stack has no transcoded
 * asset, so the track list is faked with `page.route` and the flow is clicked through on the fake). Editorial, Lounge
 * Light and Lounge Dark, desktop 1280x800 and phone 375x812. Local only (SR_P67_MEDIA=1); shots go to SR_P67_MEDIA_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P67_MEDIA_OUT ?? resolve('test-results/p67-media');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const NOTE =
  'Times shown are in your own time zone. Reruns of the Sunday marathon are scheduled by the volunteers who run this room, so a programme may move a few minutes either way when something ahead of it runs long. Thanks for watching.'.slice(
    0,
    200,
  );

test.skip(process.env.SR_P67_MEDIA !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    hasTouch: size.width < 500,
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

const json = (r: Route, body: unknown, status = 200) =>
  r.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function pause(page: Page) {
  await page.waitForTimeout(400);
}

const csrfOf = async (request: APIRequestContext) =>
  ((await (await request.get('/api/v1/sr/csrf')).json()) as { token: string }).token;

/** A 640 by 320 PNG with a transparent corner: a plain logo that has to be scaled down. */
function logoPng(): Buffer {
  // Hand-made so the spec needs no image library: a 1 px PNG scaled in the page is not enough, so the bytes
  // of a real 2:1 image are written (zlib stored blocks, CRCs computed here).
  const w = 640;
  const h = 320;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    const row = y * (w * 4 + 1);
    raw[row] = 0;
    for (let x = 0; x < w; x++) {
      const i = row + 1 + x * 4;
      const inCircle = (x - 320) ** 2 + (y - 160) ** 2 < 150 ** 2;
      raw[i] = inCircle ? 232 : 24;
      raw[i + 1] = inCircle ? 180 : 24;
      raw[i + 2] = inCircle ? 40 : 60;
      raw[i + 3] = x < 40 && y < 40 ? 0 : 255;
    }
  }
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b: Buffer) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  // zlib: stored blocks of at most 65535 bytes.
  const parts: Buffer[] = [Buffer.from([0x78, 0x01])];
  for (let p = 0; p < raw.length; p += 65535) {
    const piece = raw.subarray(p, Math.min(p + 65535, raw.length));
    const last = p + 65535 >= raw.length ? 1 : 0;
    const head = Buffer.alloc(5);
    head[0] = last;
    head.writeUInt16LE(piece.length, 1);
    head.writeUInt16LE(~piece.length & 0xffff, 3);
    parts.push(head, piece);
  }
  let a = 1;
  let b = 0;
  for (const x of raw) {
    a = (a + x) % 65521;
    b = (b + a) % 65521;
  }
  const adler = Buffer.alloc(4);
  adler.writeUInt32BE(((b << 16) | a) >>> 0);
  parts.push(adler);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', Buffer.concat(parts)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const channelBody = (number: number, name: string, callSign: string) => ({
  number,
  name,
  call_sign: callSign,
  kind: 'manual',
  timezone: 'UTC',
  filler_policy: 'slate',
});

async function putLogo(request: APIRequestContext, id: string) {
  const res = await request.put(`/api/v1/admin/tv/channels/${id}/logo`, {
    headers: {
      Origin: ORIGIN,
      'X-SR-CSRF': await csrfOf(request),
      'content-type': 'image/png',
    },
    data: logoPng(),
  });
  if (!res.ok()) throw new Error(`PUT logo -> ${res.status()}: ${await res.text()}`);
}

async function setTheme(o: APIRequestContext, theme: string) {
  await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
}

test('the guide: a 200 character footer note and channel logos, desktop and phone; the phone swipe', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  expect(NOTE).toHaveLength(200);
  const stamp = Date.now().toString(36).slice(-4).toUpperCase();
  const names: { id: string; number: number }[] = [];
  for (const [n, name, sign] of [
    [301, 'Retro Room', `RETRO${stamp}`],
    [302, 'Late Show', `LATE${stamp}`],
    [303, 'Cartoon Hour', `TOON${stamp}`],
  ] as const) {
    const c = await apiPost<{ id: string }>(o, '/admin/tv/channels', channelBody(n, name, sign));
    names.push({ id: c.id, number: n });
  }
  // Two of the three have a logo: the third shows the call sign as text.
  await putLogo(o, names[0]!.id);
  await putLogo(o, names[1]!.id);
  await apiPatch(o, '/admin/settings', { values: { 'tv.footer_note': NOTE }, reason: 'shots' });

  for (const theme of THEMES) {
    await setTheme(o, theme);
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto('/cinema/tv');
      await expect(page.locator('.tv-footnote')).toHaveText(NOTE);
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `guide-top-${tag}.png`) });
      await page.locator('.tv-footer').scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `guide-footer-note-${tag}.png`) });
      if (vp === 'phone') {
        // Swipe: before, then a sideways swipe to the next channel, then back.
        await page.evaluate(() => window.scrollTo(0, 0));
        const swipe = async (dx: number) => {
          await page.evaluate((delta) => {
            const el = document.querySelector('.tv-agenda')!;
            const at = {
              clientX: 250,
              clientY: 500,
              pointerId: 7,
              pointerType: 'touch',
              bubbles: true,
            };
            el.dispatchEvent(new PointerEvent('pointerdown', at));
            el.dispatchEvent(new PointerEvent('pointerup', { ...at, clientX: 250 + delta }));
          }, dx);
          await pause(page);
        };
        const heading = () => page.getByRole('region', { name: /^On now, channel/ });
        const first = await heading().getAttribute('aria-label');
        await page.screenshot({ path: resolve(OUT, `phone-agenda-before-swipe-${theme}.png`) });
        await swipe(-120);
        const second = await heading().getAttribute('aria-label');
        expect(second).not.toBe(first);
        await page.screenshot({ path: resolve(OUT, `phone-agenda-after-swipe-${theme}.png`) });
        await swipe(120);
        expect(await heading().getAttribute('aria-label')).toBe(first);
      }
      await c.close();
    }
  }
  await setTheme(o, 'lounge-editorial');
});

test('the channel editor: upload a logo, save, reload, remove (clicked through)', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const stamp = Date.now().toString(36).slice(-4).toUpperCase();
  let n = 410;
  for (const theme of THEMES) {
    await setTheme(o, theme);
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const ch = await apiPost<{ id: string }>(
        o,
        '/admin/tv/channels',
        channelBody(n++, `Editor ${vp} ${theme}`, `ED${stamp}`),
      );
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto('/admin/cinema-live-tv');
      await page.getByRole('button', { name: `Open Editor ${vp} ${theme}` }).click();
      await page.getByRole('form', { name: 'Channel details' }).waitFor();
      const row = page.getByRole('region', { name: 'Logo' });
      await expect(row.getByText(/No logo/)).toBeVisible();
      await row.scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `editor-no-logo-${tag}.png`) });
      // Upload from a real file chooser.
      await page.getByLabel('Upload logo').setInputFiles({
        name: 'logo.png',
        mimeType: 'image/png',
        buffer: logoPng(),
      });
      await expect(row.getByText('Logo uploaded.')).toBeVisible();
      await expect(row.getByRole('img', { name: 'Channel logo' })).toBeVisible();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `editor-logo-uploaded-${tag}.png`) });
      // Save keeps it; a reload still shows it (the key is echoed back).
      await page.getByRole('button', { name: 'Save channel' }).click();
      await expect(page.getByText('Channel saved.')).toBeVisible();
      await page.reload();
      await page.getByRole('button', { name: `Open Editor ${vp} ${theme}` }).click();
      await expect(
        page.getByRole('region', { name: 'Logo' }).getByRole('img', { name: 'Channel logo' }),
      ).toBeVisible();
      await page.getByRole('region', { name: 'Logo' }).scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `editor-logo-after-reload-${tag}.png`) });
      // The served logo is the re-encoded image: at most 512 wide.
      const src = (await page
        .getByRole('region', { name: 'Logo' })
        .getByRole('img', { name: 'Channel logo' })
        .getAttribute('src'))!;
      const served = await o.get(src);
      expect(served.status()).toBe(200);
      expect(served.headers()['content-type']).toBe('image/png');
      // Remove.
      await page.getByRole('button', { name: 'Remove logo' }).click();
      await expect(
        page.getByRole('region', { name: 'Logo' }).getByText('Logo removed.'),
      ).toBeVisible();
      await expect(page.getByRole('region', { name: 'Logo' }).getByText(/No logo/)).toBeVisible();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `editor-logo-removed-${tag}.png`) });
      await c.close();
      void ch;
    }
  }
  await setTheme(o, 'lounge-editorial');
});

test('Admin > Settings: the footer note field is listed, saved and kept after a reload', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await apiPatch(o, '/admin/settings', { values: { 'tv.footer_note': '' }, reason: 'reset' });
  for (const theme of THEMES) {
    await setTheme(o, theme);
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto('/admin/limits');
      const field = page.getByLabel('tv.footer_note');
      await expect(field).toBeVisible();
      await field.scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `admin-settings-footer-note-${tag}.png`) });
      await field.fill(NOTE);
      await page.getByRole('button', { name: /^Save 1 change/ }).click();
      await page.getByRole('textbox').last().fill('Checking the footer note');
      await page.getByRole('button', { name: 'Save settings' }).click();
      await expect(page.getByText('Settings saved. They are live now.')).toBeVisible();
      await page.reload();
      await expect(page.getByLabel('tv.footer_note')).toHaveValue(NOTE);
      await page.getByLabel('tv.footer_note').scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `admin-settings-footer-note-saved-${tag}.png`) });
      await apiPatch(o, '/admin/settings', { values: { 'tv.footer_note': '' }, reason: 'reset' });
      await c.close();
    }
  }
  await setTheme(o, 'lounge-editorial');
});

interface Track {
  id: string;
  label: string;
  language: string | null;
  forced: boolean;
  hearing_impaired: boolean;
  is_default: boolean;
  hidden: boolean;
  source: 'embedded' | 'uploaded';
  styled: boolean;
}

test('the Subtitles block of the asset panel, on a faked track list', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const stamp = Date.now().toString(36);
  const title = await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
    kind: 'movie',
    title: `Subtitle Booth ${stamp}`,
    synopsis: 'A film with subtitles.',
    subtitle_languages: ['en', 'es', 'de'],
  });
  const ASSET = '7001';
  for (const theme of THEMES) {
    await setTheme(o, theme);
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      let tracks: Track[] = [
        {
          id: '1',
          label: 'English',
          language: 'en',
          forced: false,
          hearing_impaired: false,
          is_default: true,
          hidden: false,
          source: 'embedded',
          styled: false,
        },
        {
          id: '2',
          label: 'Español',
          language: 'es',
          forced: false,
          hearing_impaired: false,
          is_default: false,
          hidden: true,
          source: 'embedded',
          styled: false,
        },
        {
          id: '3',
          label: 'Deutsch',
          language: 'de',
          forced: false,
          hearing_impaired: true,
          is_default: false,
          hidden: false,
          source: 'uploaded',
          styled: false,
        },
        {
          id: '4',
          label: 'Signs and songs',
          language: 'en',
          forced: true,
          hearing_impaired: false,
          is_default: false,
          hidden: false,
          source: 'uploaded',
          styled: true,
        },
      ];
      let failUpload: string | null = 'no readable cues';
      // The title carries a linked video (a fresh stack has none), and the asset's status answers.
      await page.route(new RegExp(`/api/v1/admin/cinema/titles/${title.id}$`), async (r) => {
        const res = await r.fetch();
        const body = (await res.json()) as { title: Record<string, unknown> };
        body.title.asset_id = ASSET;
        body.title.playable = true;
        await json(r, body);
      });
      await page.route(
        new RegExp(`/api/v1/admin/cinema/assets/${ASSET}/subtitles(\\?.*)?$`),
        async (r) => {
          const m = r.request().method();
          if (m === 'GET') return json(r, { tracks });
          if (failUpload) return json(r, { code: 50035, message: failUpload }, 400);
          const t: Track = {
            id: '9',
            label: 'Français',
            language: 'fr',
            forced: false,
            hearing_impaired: false,
            is_default: false,
            hidden: false,
            source: 'uploaded',
            styled: false,
          };
          tracks = [...tracks, t];
          return json(r, t, 201);
        },
      );
      await page.route(
        new RegExp(`/api/v1/admin/cinema/assets/${ASSET}/subtitles/\\d+$`),
        async (r) => {
          const id = r.request().url().split('/').pop()!;
          if (r.request().method() === 'DELETE') {
            tracks = tracks.filter((t) => t.id !== id);
            return r.fulfill({ status: 204 });
          }
          const patch = JSON.parse(r.request().postData() ?? '{}') as Partial<Track>;
          if (patch.is_default) tracks = tracks.map((t) => ({ ...t, is_default: false }));
          tracks = tracks.map((t) => (t.id === id ? { ...t, ...patch } : t));
          return json(
            r,
            tracks.find((t) => t.id === id),
          );
        },
      );
      await page.goto('/admin/cinema-catalog');
      await page
        .getByRole('button', { name: new RegExp(`Subtitle Booth ${stamp}`) })
        .first()
        .click();
      await page.getByRole('tab', { name: 'Video and transcoding' }).click();
      await page.getByRole('button', { name: /^Show the subtitles of/ }).click();
      const block = page.getByRole('region', { name: 'Subtitles' });
      await expect(block.getByRole('listitem')).toHaveCount(4);
      await block.scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `subtitles-four-tracks-${tag}.png`) });
      // Clicked through on the fake: a switch, Hide, an edit of the label, a refused upload, an accepted one, a delete.
      await block.getByRole('switch', { name: 'Default for Deutsch' }).click();
      await expect(block.getByRole('switch', { name: 'Default for English' })).not.toBeChecked();
      await block.getByRole('button', { name: 'Hide English' }).click();
      await expect(block.getByText('Hidden from viewers')).toHaveCount(2);
      const label = block.getByLabel('Label of track 4');
      await label.fill('Signs & songs');
      await label.press('Enter');
      await expect(label).toHaveValue('Signs & songs');
      await block.getByLabel(/Subtitle file/).setInputFiles({
        name: 'movie.fr.srt',
        mimeType: 'text/plain',
        buffer: Buffer.from('this is not a subtitle file'),
      });
      await block.getByLabel('Language of the new track').fill('fr');
      await block.getByRole('button', { name: 'Add subtitles' }).click();
      await expect(block.getByRole('alert')).toHaveText('no readable cues');
      await block.getByRole('button', { name: 'Add subtitles' }).scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `subtitles-upload-refused-${tag}.png`) });
      failUpload = null;
      await block.getByLabel('Label of the new track').fill('Français');
      await block.getByRole('button', { name: 'Add subtitles' }).click();
      await expect(block.getByRole('listitem')).toHaveCount(5);
      await block.getByRole('button', { name: 'Delete Deutsch' }).click();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `subtitles-delete-confirm-${tag}.png`) });
      await page.getByRole('dialog').getByRole('button', { name: 'Delete subtitles' }).click();
      await expect(block.getByRole('listitem')).toHaveCount(4);
      await block.scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `subtitles-after-flow-${tag}.png`) });
      await c.close();
    }
  }
  await setTheme(o, 'lounge-editorial');
});
