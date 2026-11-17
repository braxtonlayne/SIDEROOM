import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { expect, test } from '../support/fixtures';
import { serveTone, toneWav } from '../support/playerAudio';

/**
 * SEC-AUD-429 (wave 94; D-935): why does the audio element return to 0 after a seek in the test harness?
 * REPORT ONLY: this spec builds nothing and changes no page; it logs facts to `measure.json` and the console
 * (lines that start `FACT`). It NEEDS the sweep's seeded world (`e2e/.state/sweep/owner.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited): the Owner signs in from it.
 *
 * Steps (each logged): (1) the tone (duration, bytes, WAV header); (2) 1280 x 800, the default skin, the player
 * with the three tone tracks: case a `audio.currentTime = 20` set directly, case b the Seek input clicked at 50 %,
 * case c a seek to 25 % of `duration`; each sampled every 20 ms for 2 s, with `seekable` and `buffered` before and
 * after, every request and response for `e2e-tone-*.wav` (Range header, status, headers), every media event with
 * `currentTime`, and every write to `currentTime`, `src` and `load()` with its stack (an init script wraps the
 * `HTMLMediaElement.prototype` setters, so a write of the page's own code is told from the browser's own reset);
 * (3) 375 x 812 with touch and the Tidepool skin, case a and a tap on Seek; (4) a bare `new Audio()` on a neutral
 * page with no app code, same seek; (5) the variants of the tone route on that bare element (content-length on
 * both answers, a tone four times longer, the first plain request answered 206, no range support at all, all
 * together) to find the one change that stops it. No shots. Output: `measure.json` in SR_P94_SEEK_OUT.
 *
 * Local only (SR_P94_SEEK_ON=1). SR_P94_SEEK_OUT is the output folder. SR_P94_SEEK_BASE=1 is accepted (this
 * spec has no change to switch off: it runs the same facts).
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8724, Vite 5724, metrics 9424,
 * `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_E2E_BASE_URL=http://localhost:5724 SR_P94_SEEK_ON=1 \
 *   SR_P94_SEEK_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-seek \
 *   npx playwright test tests/p94-seek-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P94_SEEK_ON === '1';
const OUT =
  process.env.SR_P94_SEEK_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-seek';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p94-${n}`,
  title: `Tone ${n}`,
  artist: 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

interface Rec {
  t: number;
  what: string;
  value?: unknown;
  stack?: string[];
}
interface Ev {
  t: number;
  name: string;
  el: number;
  ct: number;
  rs: number;
  paused: boolean;
  src: string;
}
interface PW {
  __w: Rec[];
  __ev: Ev[];
  __t0: number;
  __srPlayerActions: { play(t: unknown): void; setSkin(k: string): void };
  __srPlayer: { audio: HTMLAudioElement };
}

/** Wrap the media element's `currentTime` and `src` setters and `load()` so every write is logged with a stack,
 * and record every media event (capture phase on the document) with the element's `currentTime`. */
function instrument(): void {
  const w = window as unknown as PW;
  w.__w = [];
  w.__ev = [];
  w.__t0 = performance.now();
  const stack = () =>
    (new Error().stack ?? '')
      .split('\n')
      .slice(2, 9)
      .map((l) => l.trim().replace(/^at /, '').slice(0, 200));
  const proto = HTMLMediaElement.prototype;
  const ids = new WeakMap<object, number>();
  let nextId = 1;
  const idOf = (e: object) => {
    if (!ids.has(e)) ids.set(e, nextId++);
    return ids.get(e)!;
  };
  for (const prop of ['currentTime', 'src'] as const) {
    const d = Object.getOwnPropertyDescriptor(proto, prop);
    if (!d?.set || !d.get) continue;
    const set = d.set;
    Object.defineProperty(proto, prop, {
      configurable: true,
      enumerable: d.enumerable,
      get: d.get,
      set(this: HTMLMediaElement, v: unknown) {
        w.__w.push({
          t: Math.round(performance.now() - w.__t0),
          what: `set ${prop} (el ${idOf(this)})`,
          value: v,
          stack: stack(),
        });
        set.call(this, v);
      },
    });
  }
  for (const m of ['load', 'play', 'pause'] as const) {
    const orig = proto[m] as (this: HTMLMediaElement) => unknown;
    Object.defineProperty(proto, m, {
      configurable: true,
      writable: true,
      value(this: HTMLMediaElement) {
        w.__w.push({
          t: Math.round(performance.now() - w.__t0),
          what: `${m}() (el ${idOf(this)})`,
          stack: stack(),
        });
        return orig.call(this);
      },
    });
  }
  const names = [
    'seeking',
    'seeked',
    'timeupdate',
    'emptied',
    'loadstart',
    'durationchange',
    'ended',
    'error',
    'loadedmetadata',
    'canplay',
    'play',
    'pause',
    'playing',
    'waiting',
    'stalled',
    'suspend',
    'abort',
    'ratechange',
  ];
  for (const n of names)
    document.addEventListener(
      n,
      (e) => {
        const el = e.target as HTMLMediaElement;
        if (!(el instanceof HTMLMediaElement)) return;
        w.__ev.push({
          t: Math.round(performance.now() - w.__t0),
          name: n,
          el: idOf(el),
          ct: Number(el.currentTime.toFixed(3)),
          rs: el.readyState,
          paused: el.paused,
          src: el.currentSrc.slice(-24),
        });
      },
      true,
    );
}

interface Net {
  req: { t: number; url: string; method: string; range: string | null }[];
  res: { t: number; url: string; status: number; headers: Record<string, string> }[];
}
function watchNet(page: Page): Net {
  const net: Net = { req: [], res: [] };
  const t0 = Date.now();
  page.on('request', async (r) => {
    if (!/e2e-tone-/.test(r.url())) return;
    const h = await r.allHeaders().catch(() => ({}) as Record<string, string>);
    net.req.push({
      t: Date.now() - t0,
      url: r.url().slice(-18),
      method: r.method(),
      range: h['range'] ?? null,
    });
  });
  page.on('response', async (r) => {
    if (!/e2e-tone-/.test(r.url())) return;
    const h = await r.allHeaders().catch(() => ({}) as Record<string, string>);
    net.res.push({ t: Date.now() - t0, url: r.url().slice(-18), status: r.status(), headers: h });
  });
  return net;
}

/** Run in the page: sample the element's `currentTime` every 20 ms for `ms`. */
function sampleIn(args: { sel: string | null; ms: number }): Promise<[number, number][]> {
  const w = window as unknown as PW;
  const a =
    args.sel === '__bare'
      ? (window as unknown as { __bare: HTMLAudioElement }).__bare
      : w.__srPlayer.audio;
  return new Promise((res) => {
    const out: [number, number][] = [];
    const t0 = performance.now();
    const id = setInterval(() => {
      out.push([Math.round(performance.now() - t0), Number(a.currentTime.toFixed(3))]);
      if (performance.now() - t0 >= args.ms) {
        clearInterval(id);
        res(out);
      }
    }, 20);
  });
}

function summarize(s: [number, number][], target: number) {
  const after = s.filter(([, v]) => v >= 0);
  const firstBack = after.find(([, v]) => v < target - 1);
  return {
    n: s.length,
    first: s.slice(0, 6),
    last: s.at(-1),
    min: Math.min(...s.map(([, v]) => v)),
    max: Math.max(...s.map(([, v]) => v)),
    snappedBack: firstBack !== undefined,
    firstBelowTarget: firstBack ?? null,
  };
}

function stateIn(k: string) {
  const w = window as unknown as PW & { __bare?: HTMLAudioElement };
  const a = k === 'bare' ? w.__bare! : w.__srPlayer.audio;
  const rng = (r: TimeRanges) =>
    Array.from({ length: r.length }, (_, i) =>
      [r.start(i), r.end(i)].map((x) => Number(x.toFixed(2))),
    );
  return {
    kind: k,
    currentTime: Number(a.currentTime.toFixed(3)),
    duration: a.duration,
    readyState: a.readyState,
    networkState: a.networkState,
    paused: a.paused,
    ended: a.ended,
    loop: a.loop,
    seeking: a.seeking,
    src: a.currentSrc.slice(-24),
    seekable: { count: a.seekable.length, ranges: rng(a.seekable) },
    buffered: { count: a.buffered.length, ranges: rng(a.buffered) },
  };
}
const readPlayer = (page: Page) => page.evaluate(stateIn, 'player');
const readBare = (page: Page) => page.evaluate(stateIn, 'bare');

type Report = Record<string, unknown>;

async function flush(report: Report): Promise<void> {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
}

async function mark(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as PW;
    w.__w.length = 0;
    w.__ev.length = 0;
    w.__t0 = performance.now();
  });
}

async function openPlayer(page: Page, skin: string | null): Promise<void> {
  await page.goto('/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(() => (window as unknown as Partial<PW>).__srPlayerActions);
  await page.evaluate((t) => (window as unknown as PW).__srPlayerActions.play(t), TRACKS);
  await page.waitForFunction(() => (window as unknown as Partial<PW>).__srPlayer);
  await expect
    .poll(() => page.evaluate(() => (window as unknown as PW).__srPlayer.audio.currentTime), {
      timeout: 20_000,
    })
    .toBeGreaterThan(0.4);
  if (skin) {
    await page.evaluate((s) => (window as unknown as PW).__srPlayerActions.setSkin(s), skin);
    await expect(page.locator(`.sr-ply[data-skin="${skin}"]`)).toBeVisible();
  }
  await page.waitForFunction(
    () => (window as unknown as PW).__srPlayer.audio.readyState === 4,
    null,
    {
      timeout: 20_000,
    },
  );
  await page.waitForTimeout(800);
}

/** One seek case on the player: `how` does the seek; then 2 s of samples and the logs. */
async function playerCase(
  page: Page,
  net: Net,
  label: string,
  target: number,
  how: () => Promise<void>,
): Promise<Report> {
  await mark(page);
  net.req.length = 0;
  net.res.length = 0;
  const before = await readPlayer(page);
  const t0 = Date.now();
  await how();
  const samples = await page.evaluate(sampleIn, { sel: null, ms: 2000 });
  const after = await readPlayer(page);
  const writes = await page.evaluate(() => (window as unknown as PW).__w);
  const ev = await page.evaluate(() => (window as unknown as PW).__ev);
  const out: Report = {
    label,
    target,
    howTookMs: Date.now() - t0,
    summary: summarize(samples, target),
    before,
    after,
    requests: net.req,
    responses: net.res,
    writes,
    events: ev.filter((e) => e.name !== 'timeupdate').slice(0, 60),
    timeupdates: ev.filter((e) => e.name === 'timeupdate').slice(0, 12),
  };
  console.log(
    `FACT ${label}: target ${target}, snappedBack ${(out.summary as { snappedBack: boolean }).snappedBack}, min ${(out.summary as { min: number }).min}, max ${(out.summary as { max: number }).max}, writes ${writes.length}, requests ${net.req.length}`,
  );
  return out;
}

const seekInput = (page: Page) =>
  page.locator('.sr-ply input[type="range"][aria-label="Seek"]:visible').first();

async function clickAt(page: Page, fx: number, touch: boolean): Promise<void> {
  const box = await seekInput(page).boundingBox();
  if (!box) throw new Error('no Seek box');
  const x = box.x + box.width * fx;
  const y = box.y + box.height / 2;
  if (touch) await page.touchscreen.tap(x, y);
  else await page.mouse.click(x, y);
}

/** The tone route of the variants: registered after `serveTone`, so it wins. */
interface Variant {
  name: string;
  seconds: number;
  contentLength: boolean;
  plain206: boolean;
  noRanges: boolean;
}
async function variantRoute(page: Page, v: Variant, file: string): Promise<void> {
  const tone = toneWav(v.seconds);
  await page.route(`**/${file}`, async (route: Route) => {
    const hdr = (await route.request().headerValue('range')) ?? '';
    const range = /bytes=(\d+)-(\d*)/.exec(hdr);
    const base: Record<string, string> = { 'content-type': 'audio/wav' };
    if (!v.noRanges) base['accept-ranges'] = 'bytes';
    if (v.noRanges || (!range && !v.plain206)) {
      await route.fulfill({
        status: 200,
        body: tone,
        headers: { ...base, ...(v.contentLength ? { 'content-length': String(tone.length) } : {}) },
      });
      return;
    }
    const from = range ? Number(range[1]) : 0;
    const to = range && range[2] ? Math.min(Number(range[2]), tone.length - 1) : tone.length - 1;
    const body = tone.subarray(from, to + 1);
    await route.fulfill({
      status: 206,
      body,
      headers: {
        ...base,
        'content-range': `bytes ${from}-${to}/${tone.length}`,
        ...(v.contentLength ? { 'content-length': String(body.length) } : {}),
      },
    });
  });
}

/** On the neutral page: a bare Audio for `file`, played, sought to `target`, sampled for 2 s. */
async function bareCase(page: Page, net: Net, label: string, file: string, target: number) {
  net.req.length = 0;
  net.res.length = 0;
  await page.evaluate(async (f) => {
    const w = window as unknown as { __bare?: HTMLAudioElement };
    w.__bare?.pause();
    const a = new Audio(f);
    w.__bare = a;
    await a.play();
    await new Promise<void>((ok) => {
      const tick = () => (a.readyState >= 4 && a.currentTime > 0.3 ? ok() : setTimeout(tick, 50));
      tick();
    });
  }, `/${file}`);
  await mark(page);
  net.req.length = 0;
  net.res.length = 0;
  const before = await readBare(page);
  await page.evaluate((t) => {
    (window as unknown as { __bare: HTMLAudioElement }).__bare.currentTime = t;
  }, target);
  const samples = await page.evaluate(sampleIn, { sel: '__bare', ms: 2000 });
  const after = await readBare(page);
  const writes = await page.evaluate(() => (window as unknown as PW).__w);
  const ev = await page.evaluate(() => (window as unknown as PW).__ev);
  const summary = summarize(samples, target);
  console.log(
    `FACT ${label}: target ${target}, snappedBack ${summary.snappedBack}, min ${summary.min}, max ${summary.max}, requests ${net.req.length}`,
  );
  return {
    label,
    file,
    target,
    summary,
    before,
    after,
    requests: net.req,
    responses: net.res.map((r) => ({
      status: r.status,
      range: r.headers['content-range'],
      len: r.headers['content-length'],
      ar: r.headers['accept-ranges'],
    })),
    writes: writes.map((w) => `${w.what}=${String(w.value)}`),
    events: ev.filter((e) => e.name !== 'timeupdate').slice(0, 40),
  };
}

test('SEC-AUD-429: the seek snap-back, isolated (report only)', async ({ browser }) => {
  const report: Report = {};
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const stateJson = await ownerCtx.storageState();
  await ownerCtx.close();

  // 1. The tone.
  const tone = toneWav(40);
  report.tone = {
    bytes: tone.length,
    riff: tone.toString('ascii', 0, 4),
    fmt: tone.toString('ascii', 8, 16),
    format: tone.readUInt16LE(20),
    channels: tone.readUInt16LE(22),
    rate: tone.readUInt32LE(24),
    byteRate: tone.readUInt32LE(28),
    bits: tone.readUInt16LE(34),
    dataLen: tone.readUInt32LE(40),
    riffLen: tone.readUInt32LE(4),
    seconds: tone.readUInt32LE(40) / tone.readUInt32LE(28),
    twentyIsInside: 20 < tone.readUInt32LE(40) / tone.readUInt32LE(28),
  };
  console.log(`FACT 1 tone: ${JSON.stringify(report.tone)}`);
  await flush(report);

  // 2. 1280 x 800, default skin, the player.
  {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: 1280, height: 800 },
      storageState: stateJson,
      bypassCSP: true,
    });
    await ctx.addInitScript(instrument);
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await serveTone(page);
    const net = watchNet(page);
    await openPlayer(page, null);
    report.desktopStart = await readPlayer(page);
    report.desktopSeekInputs = await page
      .locator('.sr-ply input[type="range"]')
      .evaluateAll((els) =>
        els.map(
          (e) =>
            `${e.getAttribute('aria-label')} ${(e as HTMLInputElement).max} vis ${(e as HTMLElement).offsetParent !== null}`,
        ),
      );
    report.skinAtStart = await page.locator('.sr-ply').first().getAttribute('data-skin');
    const dur = (report.desktopStart as { duration: number }).duration;
    report.a = await playerCase(page, net, 'a: currentTime = 20 (1280)', 20, async () => {
      await page.evaluate(() => {
        (window as unknown as PW).__srPlayer.audio.currentTime = 20;
      });
    });
    await flush(report);
    // Restart from the start so b is not affected by a.
    await page.evaluate(() => {
      (window as unknown as PW).__srPlayer.audio.currentTime = 2;
    });
    await page.waitForTimeout(1500);
    report.b = await playerCase(
      page,
      net,
      'b: Seek input clicked at 50% (1280)',
      dur / 2,
      async () => {
        await clickAt(page, 0.5, false);
      },
    );
    await flush(report);
    await page.evaluate(() => {
      (window as unknown as PW).__srPlayer.audio.currentTime = 2;
    });
    await page.waitForTimeout(1500);
    report.c = await playerCase(
      page,
      net,
      'c: currentTime = 25% of duration (1280)',
      dur / 4,
      async () => {
        await page.evaluate((t) => {
          (window as unknown as PW).__srPlayer.audio.currentTime = t;
        }, dur / 4);
      },
    );
    await flush(report);
    await ctx.close();
  }

  // 3. 375 touch, Tidepool.
  {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: 375, height: 812 },
      storageState: stateJson,
      bypassCSP: true,
      userAgent: PHONE_UA,
      isMobile: true,
      hasTouch: true,
    });
    await ctx.addInitScript(instrument);
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await serveTone(page);
    const net = watchNet(page);
    await openPlayer(page, 'tidepool');
    const dur = (await readPlayer(page)).duration as number;
    report.phoneA = await playerCase(
      page,
      net,
      'a: currentTime = 20 (375 tidepool)',
      20,
      async () => {
        await page.evaluate(() => {
          (window as unknown as PW).__srPlayer.audio.currentTime = 20;
        });
      },
    );
    await page.evaluate(() => {
      (window as unknown as PW).__srPlayer.audio.currentTime = 2;
    });
    await page.waitForTimeout(1500);
    report.phoneB = await playerCase(
      page,
      net,
      'b: tap on Seek at 50% (375 tidepool)',
      dur / 2,
      async () => {
        await page.locator('.sr-ply input.tp-seek').first().scrollIntoViewIfNeeded();
        await clickAt(page, 0.5, true);
      },
    );
    await flush(report);
    await ctx.close();
  }

  // 4 and 5. The bare element on a neutral page (no app code), then the variants of the tone route.
  {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      storageState: stateJson,
      bypassCSP: true,
    });
    await ctx.addInitScript(instrument);
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await serveTone(page);
    await page.route('**/e2e-neutral.html', (r) =>
      r.fulfill({
        status: 200,
        contentType: 'text/html',
        body: '<!doctype html><title>n</title><p>neutral</p>',
      }),
    );
    const net = watchNet(page);
    await page.goto('/e2e-neutral.html');
    await page.mouse.click(2, 2);
    const bare: Report[] = [];
    bare.push(
      await bareCase(page, net, 'bare: serveTone as it is, currentTime = 20', 'e2e-tone-1.wav', 20),
    );
    bare.push(
      await bareCase(page, net, 'bare: serveTone as it is, currentTime = 10', 'e2e-tone-2.wav', 10),
    );
    report.bare = bare;
    await flush(report);
    const variants: Variant[] = [
      {
        name: 'content-length on both answers',
        seconds: 40,
        contentLength: true,
        plain206: false,
        noRanges: false,
      },
      {
        name: 'tone 4x longer (160 s)',
        seconds: 160,
        contentLength: false,
        plain206: false,
        noRanges: false,
      },
      {
        name: 'first plain request answered 206 with content-range',
        seconds: 40,
        contentLength: false,
        plain206: true,
        noRanges: false,
      },
      {
        name: 'no range support at all (200, no accept-ranges)',
        seconds: 40,
        contentLength: false,
        plain206: false,
        noRanges: true,
      },
      {
        name: 'all three: content-length, 160 s, plain 206',
        seconds: 160,
        contentLength: true,
        plain206: true,
        noRanges: false,
      },
    ];
    const out: Report[] = [];
    let n = 10;
    for (const v of variants) {
      const file = `e2e-tone-${n++}.wav`;
      await variantRoute(page, v, file);
      const r = await bareCase(page, net, `variant: ${v.name}`, file, 20);
      out.push({ ...r, variant: v });
      await flush({ ...report, variants: out });
    }
    report.variants = out;
    await ctx.close();
  }
  await flush(report);
});
