import { expect, type Page } from '@playwright/test';

/**
 * Helpers for the floating player's browser tests (MUS-PLY-001): a generated WAV tone served with byte ranges
 * (the way a presigned bucket URL answers), and the dev-build handles the app exposes
 * (`window.__srPlayerActions`, `window.__srPlayer`) for starting playback and reading the audio element.
 */

/** A mono 8 kHz 8-bit WAV of a 440 Hz tone. */
export function toneWav(seconds: number): Buffer {
  const rate = 8000;
  const n = rate * seconds;
  const buf = Buffer.alloc(44 + n);
  buf.write('RIFF', 0);
  buf.writeUInt32LE(36 + n, 4);
  buf.write('WAVEfmt ', 8);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(rate, 24);
  buf.writeUInt32LE(rate, 28);
  buf.writeUInt16LE(1, 32);
  buf.writeUInt16LE(8, 34);
  buf.write('data', 36);
  buf.writeUInt32LE(n, 40);
  for (let i = 0; i < n; i++)
    buf[44 + i] = 128 + Math.round(60 * Math.sin((2 * Math.PI * 440 * i) / rate));
  return buf;
}

const TONE = toneWav(40);

export async function serveTone(page: Page): Promise<void> {
  await page.route('**/e2e-tone-*.wav', async (route) => {
    const range = /bytes=(\d+)-(\d*)/.exec((await route.request().headerValue('range')) ?? '');
    if (!range) {
      await route.fulfill({
        status: 200,
        body: TONE,
        headers: { 'content-type': 'audio/wav', 'accept-ranges': 'bytes' },
      });
      return;
    }
    const from = Number(range[1]);
    const to = range[2] ? Math.min(Number(range[2]), TONE.length - 1) : TONE.length - 1;
    await route.fulfill({
      status: 206,
      body: TONE.subarray(from, to + 1),
      headers: {
        'content-type': 'audio/wav',
        'accept-ranges': 'bytes',
        'content-range': `bytes ${from}-${to}/${TONE.length}`,
      },
    });
  });
}

export const LONG_TITLE = 'Midnight Drive (Extended Mix)';
export const LONG_ARTIST = 'Neon Coast';

export const TRACKS = [1, 2, 3].map((n) => ({
  id: `e2e-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? LONG_ARTIST : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

interface Handle {
  audio: HTMLAudioElement;
}

/** Queue the tracks and wait until the audio element advances. Call after a signed-in app page has loaded. */
export async function startPlaying(page: Page): Promise<void> {
  await page.waitForFunction(() => document.readyState === 'complete');
  // The browser lets sound start only after the member has touched the page.
  await page.mouse.click(2, 2);
  await page.waitForFunction(
    () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
  );
  await page.evaluate((tracks) => {
    (window as unknown as { __srPlayerActions: { play(t: unknown): void } }).__srPlayerActions.play(
      tracks,
    );
  }, TRACKS);
  await page.waitForFunction(() => (window as unknown as { __srPlayer?: Handle }).__srPlayer);
  await expect.poll(() => currentTime(page), { timeout: 15_000 }).toBeGreaterThan(0.4);
}

export const currentTime = (page: Page) =>
  page.evaluate(() => (window as unknown as { __srPlayer: Handle }).__srPlayer.audio.currentTime);
export const isPaused = (page: Page) =>
  page.evaluate(() => (window as unknown as { __srPlayer: Handle }).__srPlayer.audio.paused);

/** Every skin the player has (MUS-PLY-001's three, then MUS-SKN-001's backdrop skins). */
export const ALL_SKINS = [
  'retro',
  'pill',
  'library',
  'valve',
  'tidepool',
  'circuit',
  'study',
  'stack',
] as const;
export type SkinId = (typeof ALL_SKINS)[number];

export async function setSkin(page: Page, skin: SkinId): Promise<void> {
  await page.evaluate(
    (s) =>
      (
        window as unknown as { __srPlayerActions: { setSkin(k: string): void } }
      ).__srPlayerActions.setSkin(s),
    skin,
  );
  await expect(page.locator(`.sr-ply[data-skin="${skin}"]`)).toBeVisible();
}

/** Move to another route without reloading: the router follows history events like a clicked link. */
export async function goInApp(page: Page, path: string): Promise<void> {
  await page.evaluate((p) => {
    window.history.pushState({}, '', p);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, path);
  await expect(page).toHaveURL(new RegExp(`${path.replace(/[/]/g, '\\/')}$`));
}

export async function expectAdvancing(page: Page): Promise<void> {
  const before = await currentTime(page);
  await expect.poll(() => currentTime(page), { timeout: 10_000 }).toBeGreaterThan(before + 0.5);
  expect(await isPaused(page)).toBe(false);
}

export interface Fit {
  viewport: { width: number; height: number };
  panel: { left: number; right: number; top: number; bottom: number } | null;
  /** scrollWidth - clientWidth of the panel. */
  scrollOverflow: number;
  /** Descendants (except the ticker's own scrolling text) whose box leaves the panel's box. */
  escaping: string[];
}

/**
 * Where the player sits and whether anything in it is clipped: the `.sr-ply` box must be inside the viewport,
 * the panel must not scroll horizontally, and no descendant other than the ticker text (which clips inside its
 * own box) may end past the panel's edge.
 */
export function measureFit(page: Page): Promise<Fit> {
  return page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const vh = document.documentElement.clientHeight;
    const el = document.querySelector<HTMLElement>('.sr-ply');
    if (!el)
      return { viewport: { width: vw, height: vh }, panel: null, scrollOverflow: 0, escaping: [] };
    const r = el.getBoundingClientRect();
    const escaping: string[] = [];
    for (const d of Array.from(el.querySelectorAll<HTMLElement>('*'))) {
      if (d.classList.contains('sr-ply-ticker-text') || d.closest('.sr-ply-pane')) continue;
      const b = d.getBoundingClientRect();
      if (b.width === 0 && b.height === 0) continue;
      // What is visible of the box: an ancestor inside the panel that clips its overflow (a backdrop skin's gel
      // wing around a scrolling wave, say) cuts the box down before it can reach the panel's edge.
      let left = b.left;
      let right = b.right;
      for (let a = d.parentElement; a && a !== el; a = a.parentElement) {
        if (getComputedStyle(a).overflowX === 'visible') continue;
        const ar = a.getBoundingClientRect();
        left = Math.max(left, ar.left);
        right = Math.min(right, ar.right);
      }
      if (right <= left) continue;
      if (right > r.right + 0.5 || left < r.left - 0.5)
        escaping.push(`${d.tagName.toLowerCase()}.${d.getAttribute('class') ?? ''}`);
    }
    return {
      viewport: { width: vw, height: vh },
      panel: { left: r.left, right: r.right, top: r.top, bottom: r.bottom },
      scrollOverflow: el.scrollWidth - el.clientWidth,
      escaping,
    };
  });
}
