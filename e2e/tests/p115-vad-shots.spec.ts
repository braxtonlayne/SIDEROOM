import { mkdirSync, mkdtempSync, readFileSync, rmSync, existsSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { chromium } from '@playwright/test';
import type { Browser, BrowserContext, Locator, Page, Request } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * Wave 115, vadshots (VOX-CLI-002; D-1048 to D-1052): the checker of the Silero voice detector. It changes no app
 * file. Every part runs as its own job (SR_P115_VAD_PART), each under six minutes; every part that joins a call needs
 * the server run with `--server-env SR_VOICE__ENABLED=true`. Chromium only (fake microphone flags): the other two
 * browsers are skipped and the handoff says so.
 *
 *  1  The settings UI: User settings > Voice & Video, the card `#us-voice-mode`, 1280 x 800 Lounge Light and Dark and
 *     375 x 812 touch Light: every control's box (Standard chosen, compared with the base), the new control's two
 *     options, Silero chosen (status line and state, the disabled switch and slider, the explanation whole, the hit
 *     test), the keyboard (Tab to the control, arrows change it). Both phases; the base has no such control and only
 *     records that and shoots the card as it is.
 *  2  Nothing loads early (after): a fresh context with Standard; every request from the Lounge to a joined call is
 *     recorded: none names the model, the wasm or the runtime's chunk. Then Silero and the microphone test: the three
 *     are requested once each from the page's own origin, with their sizes; no request to another origin; no CSP event.
 *  3  The detector on a fake microphone (after): Chromium's beep and a generated silence (and a speech clip if
 *     `e2e/fixtures/vad/speech-16k.wav` exists), engines Standard and Silero, 20 s each: frames, speech frames,
 *     speaking state, mean and worst ms per frame; the page's JS heap before and after.
 *  4  Falling back (after): the model answers 404, then the wasm is aborted: status `fallback`, one notice, the beeps
 *     still open the gate (the energy gate took over), `fallbackReason` set; the loading state is caught with a
 *     delayed model; un-routed, leave and join again: Silero loads (the setting was kept).
 *  5  E2EE (after): a call with Silero `ready` reads as a call with Standard (the E2EE snapshot, the connection panel)
 *     and the speaking state works.
 *
 * Switches: SR_P115_VAD_ON=1, SR_P115_VAD_PHASE=base|after, SR_P115_VAD_PART=1..5, SR_P115_VAD_OUT=<folder>.
 * Optional: SR_P115_VAD_RUNTIME=<regex source> for the runtime's JavaScript chunk name (default below).
 * Shots and `figures-<phase>.json` (part 1) or `figures-<phase>-part<N>.json` go to OUT; base shots carry `base-`.
 *
 * The commander's commands, whole (one per part; parts 2 to 5 pass --server-env SR_VOICE__ENABLED=true as the runner
 * takes it):
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p115-integ/apps/web/dist --server-env SR_VOICE__ENABLED=true --env SR_P115_VAD_ON=1 --env SR_P115_VAD_PHASE=after --env SR_P115_VAD_PART=1 --env SR_P115_VAD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p115-vad -- tests/p115-vad-shots.spec.ts
 */
const ON = process.env.SR_P115_VAD_ON === '1';
const PHASE = process.env.SR_P115_VAD_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P115_VAD_PART ?? '1';
const OUT =
  process.env.SR_P115_VAD_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p115-vad';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const SPEECH = resolve(repoRoot, 'e2e', 'fixtures', 'vad', 'speech-16k.wav');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const STAMP = String(Date.now()).slice(-6);

/** The three names of the feature's files (brief 5c: the names contain `silero_vad` and `ort-wasm`). */
const MODEL_RE = /silero_vad/i;
const WASM_RE = /ort-wasm/i;
const RUNTIME_RE = new RegExp(process.env.SR_P115_VAD_RUNTIME ?? 'ort.wasm.bundle', 'i');
const isVadFile = (u: string) =>
  MODEL_RE.test(u) || WASM_RE.test(u) || RUNTIME_RE.test(u.split('?')[0] ?? u);

const MODEL_PATTERN = /silero_vad[^/]*$/i;
const WASM_PATTERN = /ort-wasm[^/]*$/i;

const CHROMIUM_ARGS = [
  '--use-fake-device-for-media-stream',
  '--use-fake-ui-for-media-stream',
  '--disable-features=WebRtcHideLocalIpsWithMdns',
  '--allow-loopback-in-peer-connection',
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const problems: string[] = [];
const notes: string[] = [];
const figures: Record<string, unknown> = {};
const problem = (s: string) => {
  problems.push(s);
  say(`PROBLEM ${s}`);
};
const note = (s: string) => {
  notes.push(s);
  say(`note: ${s}`);
};
const round = (n: number) => Math.round(n * 10) / 10;

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
}
const readWorld = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

// ---- Contexts ------------------------------------------------------------------------------------------------

interface CtxOpts {
  size?: { width: number; height: number };
  touch?: boolean;
  theme?: string;
  /** The device-local voice settings written before the first load, only when the store is still empty. */
  voice?: Record<string, unknown>;
  /** Block the app's service worker (it would answer the requests a route is meant to catch). */
  blockSw?: boolean;
}

async function newCtx(browser: Browser, opts: CtxOpts = {}): Promise<BrowserContext> {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    storageState: OWNER_STATE,
    viewport: opts.size ?? DESKTOP,
    permissions: ['microphone'],
    ...(opts.blockSw ? { serviceWorkers: 'block' as const } : {}),
    ...(opts.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (a: { theme: string; voice: Record<string, unknown> | null }) => {
      try {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme: a.theme, zoom: 100 } }, version: 2 }),
        );
        if (a.voice && localStorage.getItem('sr.voice.settings') === null)
          localStorage.setItem(
            'sr.voice.settings',
            JSON.stringify({ state: { settings: a.voice }, version: 1 }),
          );
      } catch {
        /* no storage */
      }
      (window as unknown as { __cspViolations: string[] }).__cspViolations = [];
      document.addEventListener('securitypolicyviolation', (e) => {
        (window as unknown as { __cspViolations: string[] }).__cspViolations.push(
          `${e.violatedDirective} ${e.blockedURI}`,
        );
      });
    },
    { theme: opts.theme ?? 'lounge-light', voice: opts.voice ?? null },
  );
  return ctx;
}

const BEEP_VOICE = { inputMode: 'vad', autoSensitivity: false, thresholdDb: -50 };

async function settle(page: Page, ms = 4000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(600);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  if (sw > cw) problem(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
  say(`shot ${shotName(name)}.png`);
}

const card = (page: Page) => page.locator('#us-voice-mode');
const engineGroup = (page: Page) => page.getByRole('radiogroup', { name: 'Voice detection' });
const silero = (page: Page) => engineGroup(page).getByRole('radio', { name: /Silero/ });
const standard = (page: Page) => engineGroup(page).getByRole('radio', { name: 'Standard' });
const statusLine = (page: Page) => page.getByTestId('vad-engine-status');

async function openVoiceSettings(page: Page) {
  await page.goto('/settings/user/voice', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await card(page).waitFor({ timeout: 30_000 });
  await settle(page);
  await card(page).scrollIntoViewIfNeeded();
}

/** Chooses the detector in the settings, as a member does. */
async function chooseEngine(page: Page, which: 'silero' | 'standard') {
  const r = which === 'silero' ? silero(page) : standard(page);
  await engineGroup(page)
    .locator('label')
    .filter({ hasText: which === 'silero' ? /Silero/ : /Standard/ })
    .click();
  await expectChecked(r);
}
async function expectChecked(r: Locator) {
  await r.waitFor({ timeout: 10_000 });
  const ok = await r
    .evaluate(
      (el) =>
        el.getAttribute('aria-checked') === 'true' || (el as HTMLInputElement).checked === true,
    )
    .catch(() => false);
  if (!ok) await r.page().waitForTimeout(400);
}

// ---- The card, measured ----------------------------------------------------------------------------------------

interface ControlBox {
  name: string;
  tag: string;
  role: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  disabled: boolean;
}

async function controlBoxes(root: Locator): Promise<ControlBox[]> {
  return root.evaluate((el) => {
    const out: ControlBox[] = [];
    const sel =
      'button, input, select, textarea, [role=radio], [role=slider], [role=switch], [role=checkbox]';
    const nameOf = (e: Element): string => {
      const fold = (s: string) => s.trim().replace(/\s+/g, ' ').slice(0, 40);
      const aria = e.getAttribute('aria-label');
      if (aria) return fold(aria);
      const by = e.getAttribute('aria-labelledby');
      if (by)
        return fold(
          by
            .split(/\s+/)
            .map((id) => document.getElementById(id)?.textContent ?? '')
            .join(' '),
        );
      const labels = (e as HTMLInputElement).labels;
      if (labels && labels.length) return fold(labels[0]?.textContent ?? '');
      return fold(e.textContent ?? '');
    };
    for (const e of Array.from(el.querySelectorAll(sel))) {
      const r = e.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      out.push({
        name: nameOf(e),
        tag: e.tagName.toLowerCase(),
        role: e.getAttribute('role'),
        x: Math.round(r.x * 10) / 10,
        y: Math.round(r.y * 10) / 10,
        width: Math.round(r.width * 10) / 10,
        height: Math.round(r.height * 10) / 10,
        disabled:
          (e as HTMLInputElement).disabled === true || e.getAttribute('aria-disabled') === 'true',
      });
    }
    return out;
  });
}

/** Text in the card whose box is narrower than its text (cut), vertical too. */
async function cutText(root: Locator): Promise<string[]> {
  return root.evaluate((el) => {
    const cut: string[] = [];
    for (const e of Array.from(
      el.querySelectorAll<HTMLElement>('p, span, label, div, h2, h3, h4'),
    )) {
      if (e.children.length > 0 && e.tagName !== 'P') continue;
      const txt = (e.textContent ?? '').trim();
      if (!txt) continue;
      if (e.scrollWidth > e.clientWidth + 1 || e.scrollHeight > e.clientHeight + 2)
        cut.push(
          `${txt.slice(0, 50)} (w ${e.scrollWidth}/${e.clientWidth}, h ${e.scrollHeight}/${e.clientHeight})`,
        );
    }
    return cut;
  });
}

interface SileroFigures {
  statusState: string | null;
  statusText: string | null;
  switchDisabled: boolean | null;
  sliderDisabled: boolean | null;
  explanation: string[];
  cut: string[];
  boxes: ControlBox[];
}

async function sileroFigures(page: Page): Promise<SileroFigures> {
  const c = card(page);
  const st = statusLine(page);
  const stCount = await st.count();
  const boxes = await controlBoxes(c);
  const sw = boxes.find(
    (b) => b.role === 'switch' || (b.tag === 'input' && /sensitiv/i.test(b.name) && b.name !== ''),
  );
  const slider = boxes.find(
    (b) => b.role === 'slider' || (b.tag === 'input' && /sensitivity$/i.test(b.name)),
  );
  const explanation = await c.locator('p').allInnerTexts();
  return {
    statusState: stCount ? await st.first().getAttribute('data-state') : null,
    statusText: stCount ? (await st.first().innerText()).trim() || null : null,
    switchDisabled: sw ? sw.disabled : null,
    sliderDisabled: slider ? slider.disabled : null,
    explanation,
    cut: await cutText(c),
    boxes,
  };
}

async function hitTest(page: Page, label: string, phone: boolean) {
  const c = card(page);
  await openCollapsed(page, c);
  const rows = await hitTestAll(page, c, phone ? {} : { reach: 'box' });
  const fails = rows.filter((r) => r.status === 'fail');
  const notReached = rows.filter((r) => r.status === 'not-reached');
  say(
    `${label}: hit test ${rows.length} rows, ${fails.length} fail, ${notReached.length} not-reached`,
  );
  for (const r of fails) {
    const t = (r.result?.points ?? []).find((p) => !p.ok)?.taker;
    problem(
      `${label}: hit test fail on ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" taker ${t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}"` : 'none'}`,
    );
  }
  for (const r of notReached)
    note(
      `${label}: hit test not-reached on "${r.name}" (${r.reason ?? ''}): probe error, not a fault`,
    );
  return { rows: rows.length, fail: fails.length, notReached: notReached.length };
}

// ---- Part 1: the settings UI -----------------------------------------------------------------------------------

async function part1(browser: Browser) {
  const configs = [
    { key: '1280-light', size: DESKTOP, theme: 'lounge-light', touch: false },
    { key: '1280-dark', size: DESKTOP, theme: 'lounge-dark', touch: false },
    { key: '375-light', size: PHONE, theme: 'lounge-light', touch: true },
  ];
  const per: Record<string, unknown> = {};
  for (const cfg of configs) {
    const ctx = await newCtx(browser, { size: cfg.size, theme: cfg.theme, touch: cfg.touch });
    const page = await ctx.newPage();
    page.setDefaultTimeout(15_000);
    try {
      await apiPatch(ctx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme: cfg.theme, zoom: 100 } },
      });
      await openVoiceSettings(page);
      const root = await page.evaluate(
        () => document.querySelector<HTMLElement>('.lng-root, [data-theme]')?.dataset.theme ?? '?',
      );
      say(`${cfg.key}: data-theme ${root}`);
      const hasControl = (await engineGroup(page).count()) > 0;
      const stdBoxes = await controlBoxes(card(page));
      const entry: Record<string, unknown> = { hasControl, theme: root, standardBoxes: stdBoxes };
      per[cfg.key] = entry;
      entry.cardBox = await card(page).evaluate((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      });
      entry.standardCut = await cutText(card(page));
      if ((entry.standardCut as string[]).length)
        problem(`${cfg.key} standard: cut text ${JSON.stringify(entry.standardCut)}`);
      await shot(page, `voice-mode-standard-${cfg.key}`);

      if (PHASE === 'base') {
        if (hasControl) problem(`${cfg.key}: the base already has a "Voice detection" control`);
        else note(`${cfg.key}: base has no "Voice detection" control (expected)`);
        per[cfg.key] = entry;
        continue;
      }

      if (!hasControl) {
        problem(`${cfg.key}: no radiogroup named "Voice detection" in #us-voice-mode`);
        per[cfg.key] = entry;
        continue;
      }
      const options = await engineGroup(page).locator('label').allInnerTexts();
      entry.options = options.map((o) => o.trim());
      if (
        options.length !== 2 ||
        !/Standard/.test(options[0] ?? '') ||
        !/Silero \(on-device model\)/.test(options[1] ?? '')
      )
        problem(
          `${cfg.key}: options are ${JSON.stringify(options)}, not Standard + Silero (on-device model)`,
        );
      entry.standardChecked = await standard(page).evaluate(
        (el) =>
          el.getAttribute('aria-checked') === 'true' || (el as HTMLInputElement).checked === true,
      );
      if (!entry.standardChecked) problem(`${cfg.key}: Standard is not the default choice`);
      entry.statusWithStandard = await statusLine(page).count();
      if (entry.statusWithStandard) problem(`${cfg.key}: a status line shows with Standard chosen`);
      entry.groupWidths = await card(page).evaluate((el) =>
        Array.from(el.querySelectorAll('[role=radiogroup]')).map((g) => {
          const r = g.getBoundingClientRect();
          return [
            g.getAttribute('aria-label'),
            Math.round(r.width * 10) / 10,
            Math.round(r.height * 10) / 10,
          ];
        }),
      );
      entry.hitStandard = await hitTest(page, `${cfg.key} standard`, cfg.touch);

      // Keyboard (desktop light): Tab reaches the control, the arrows change it.
      if (cfg.key === '1280-light') {
        await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
        await card(page).locator('[role=radiogroup]').first().getByRole('radio').first().focus();
        let tabs = 0;
        let reached = false;
        for (; tabs < 8; tabs++) {
          reached = await engineGroup(page).evaluate((g) => g.contains(document.activeElement));
          if (reached) break;
          await page.keyboard.press('Tab');
        }
        entry.keyboard = { tabsFromModeControl: tabs, reached };
        if (!reached)
          problem(
            'keyboard: Tab did not reach "Voice detection" within 8 presses from the input mode control',
          );
        else {
          await page.keyboard.press('ArrowRight');
          await page.waitForTimeout(300);
          const nowSilero = await silero(page).evaluate(
            (el) =>
              el.getAttribute('aria-checked') === 'true' ||
              (el as HTMLInputElement).checked === true,
          );
          (entry.keyboard as Record<string, unknown>).arrowRightChoosesSilero = nowSilero;
          if (!nowSilero) problem('keyboard: ArrowRight did not choose Silero');
        }
      }
      if (
        !(await silero(page).evaluate(
          (el) =>
            el.getAttribute('aria-checked') === 'true' || (el as HTMLInputElement).checked === true,
        ))
      )
        await chooseEngine(page, 'silero');
      await page.waitForTimeout(500);
      const sf = await sileroFigures(page);
      entry.silero = sf;
      if (!sf.statusState)
        problem(`${cfg.key}: no status line (data-testid="vad-engine-status") with Silero chosen`);
      else if (!['idle', 'loading', 'ready', 'fallback'].includes(sf.statusState))
        problem(
          `${cfg.key}: status state "${sf.statusState}" is not one of idle, loading, ready, fallback`,
        );
      if (sf.switchDisabled === false)
        problem(`${cfg.key}: the automatic sensitivity switch is not disabled with Silero`);
      if (sf.sliderDisabled === false)
        problem(`${cfg.key}: the sensitivity slider is not disabled with Silero`);
      if (sf.cut.length) problem(`${cfg.key} silero: cut text ${JSON.stringify(sf.cut)}`);
      await shot(page, `voice-mode-silero-${cfg.key}`);
      entry.hitSilero = await hitTest(page, `${cfg.key} silero`, cfg.touch);

      // The rest of the card, with Standard chosen, where it was in the base.
      if (cfg.key === '1280-light') {
        await chooseEngine(page, 'standard');
        await page.waitForTimeout(300);
        const back = await controlBoxes(card(page));
        entry.standardAgainBoxes = back;
        const rel = (l: ControlBox[]) =>
          l.map((x) => [x.name, x.x, x.width, x.height, round(x.y - (l[0]?.y ?? 0))].join());
        const same = JSON.stringify(rel(back)) === JSON.stringify(rel(stdBoxes));
        if (!same)
          problem(
            '1280-light: choosing Silero and back to Standard did not restore the card (boxes differ)',
          );
      }
      per[cfg.key] = entry;
    } catch (e) {
      problem(`${cfg.key}: failed (${String(e).split('\n')[0]?.slice(0, 200)})`);
    } finally {
      await ctx.close();
    }
  }
  figures.configs = per;

  // After vs base: every control the base had is where it was (the card's Standard state).
  if (PHASE === 'after') {
    const baseFile = resolve(OUT, 'figures-base.json');
    if (existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as {
        figures?: {
          configs?: Record<string, { standardBoxes?: ControlBox[]; cardBox?: { y: number } }>;
        };
      };
      for (const cfg of configs) {
        const bEntry = base.figures?.configs?.[cfg.key];
        const b = bEntry?.standardBoxes;
        const aEntry = per[cfg.key] as
          { standardBoxes?: ControlBox[]; cardBox?: { y: number } } | undefined;
        const a = aEntry?.standardBoxes;
        if (!b || !a) continue;
        const moved: string[] = [];
        const bTop = bEntry?.cardBox?.y ?? 0;
        const aTop = aEntry?.cardBox?.y ?? 0;
        for (const bb of b) {
          const m = a.find(
            (x) => x.name === bb.name && x.tag === bb.tag && Math.abs(x.y - bb.y) < 400 && true,
          );
          const same = a.filter((x) => x.name === bb.name && x.tag === bb.tag);
          const match = same.length === 1 ? same[0] : m;
          if (!match) {
            moved.push(`"${bb.name}" is gone`);
            continue;
          }
          const dx = round(match.x - bb.x);
          const dy = round(match.y - aTop - (bb.y - bTop));
          const dw = round(match.width - bb.width);
          const dh = round(match.height - bb.height);
          if (dx || dy || dw || dh) moved.push(`"${bb.name}" dx ${dx} dy ${dy} dw ${dw} dh ${dh}`);
        }
        figures[`diff-${cfg.key}`] = moved;
        if (moved.length)
          problem(
            `${cfg.key}: controls that moved with Standard chosen (after vs base): ${moved.join('; ')}`,
          );
        else say(`${cfg.key}: every control of the base is where it was`);
      }
    } else note('figures-base.json is not in OUT: no comparison');
  }
}

// ---- Calls -------------------------------------------------------------------------------------------------------

interface Stats {
  engine: string;
  state: string;
  frames: number;
  speechFrames: number;
  meanMs: number;
  maxMs: number;
  fallbackReason: string | null;
  frameMs?: number;
}

const readStats = (page: Page) =>
  page.evaluate(() => (window as unknown as { __srVadStats?: Stats }).__srVadStats ?? null);
const speakingNow = (page: Page) =>
  page.evaluate(() => document.querySelectorAll('.lng-voice-member[data-speaking]').length > 0);
const heap = (page: Page) =>
  page.evaluate(() => {
    const m = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
    return m ? m.usedJSHeapSize : null;
  });

async function voiceChannelOf(
  owner: BrowserContext,
  w: World,
): Promise<{ id: string; made: boolean }> {
  if (w.voiceChannelId) return { id: w.voiceChannelId, made: false };
  const c = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
    name: `p115vad-calls-${STAMP}`,
    type: 2,
  });
  return { id: c.id, made: true };
}

async function joinCall(page: Page, w: World, voiceId: string) {
  await page.goto(`/channels/${w.guildId}/${voiceId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 30_000,
  });
  const join = page.getByRole('button', { name: 'Join Voice' }).first();
  await join.waitFor({ timeout: 30_000 });
  await join.click();
  await page
    .getByRole('region', { name: 'Voice connection' })
    .getByText('Voice Connected')
    .waitFor({ timeout: 60_000 });
}

async function leaveCall(page: Page) {
  const b = page.getByRole('button', { name: 'Disconnect' }).first();
  await b.click({ timeout: 10_000 }).catch(() => undefined);
  await page.waitForTimeout(800);
}

/** Waits until the stats say the detector has settled (Silero: ready or fallback). */
async function waitSettled(
  page: Page,
  engine: 'energy' | 'silero',
  ms = 60_000,
): Promise<Stats | null> {
  const end = Date.now() + ms;
  let s: Stats | null = null;
  while (Date.now() < end) {
    s = await readStats(page);
    if (
      s &&
      s.engine === engine &&
      (engine === 'energy' || s.state === 'ready' || s.state === 'fallback')
    )
      return s;
    if (s && engine === 'silero' && s.engine === 'energy' && s.state === 'fallback') return s;
    await page.waitForTimeout(300);
  }
  return s;
}

interface Sample {
  seconds: number;
  onFraction: number;
  onIntervals: { from: number; to: number }[];
  statsStart: Stats | null;
  statsEnd: Stats | null;
}

/** Samples the speaking state every 200 ms for `seconds`, and the stats before and after. */
async function sample(page: Page, seconds: number): Promise<Sample> {
  const statsStart = await readStats(page);
  const t0 = Date.now();
  let on = 0;
  let n = 0;
  const intervals: { from: number; to: number }[] = [];
  let open: number | null = null;
  while (Date.now() - t0 < seconds * 1000) {
    const t = (Date.now() - t0) / 1000;
    const s = await speakingNow(page);
    n++;
    if (s) on++;
    if (s && open === null) open = t;
    if (!s && open !== null) {
      intervals.push({ from: round(open), to: round(t) });
      open = null;
    }
    await page.waitForTimeout(200);
  }
  if (open !== null) intervals.push({ from: round(open), to: seconds });
  return {
    seconds,
    onFraction: n ? on / n : 0,
    onIntervals: intervals,
    statsStart,
    statsEnd: await readStats(page),
  };
}

/** The speech / frames figures of a sample (frames counted over the sample only). */
function delta(sm: Sample) {
  const a = sm.statsStart;
  const b = sm.statsEnd;
  const frames = (b?.frames ?? 0) - (a?.frames ?? 0);
  const speech = (b?.speechFrames ?? 0) - (a?.speechFrames ?? 0);
  return { frames, speechFrames: speech, ratio: frames > 0 ? speech / frames : null };
}

/** A 16 kHz mono 16-bit WAV of zeros. */
function silenceWav(seconds: number): Buffer {
  const n = 16000 * seconds;
  const data = Buffer.alloc(n * 2);
  const h = Buffer.alloc(44);
  h.write('RIFF', 0);
  h.writeUInt32LE(36 + data.length, 4);
  h.write('WAVEfmt ', 8);
  h.writeUInt32LE(16, 16);
  h.writeUInt16LE(1, 20);
  h.writeUInt16LE(1, 22);
  h.writeUInt32LE(16000, 24);
  h.writeUInt32LE(32000, 28);
  h.writeUInt16LE(2, 32);
  h.writeUInt16LE(16, 34);
  h.write('data', 36);
  h.writeUInt32LE(data.length, 40);
  return Buffer.concat([h, data]);
}

/** Duration and the loud region (start, end in s) of a 16-bit PCM WAV; null when the file is not one this reads. */
function wavFigures(buf: Buffer): { duration: number; speech: [number, number] | null } | null {
  if (buf.toString('ascii', 0, 4) !== 'RIFF') return null;
  let pos = 12;
  let rate = 16000;
  let channels = 1;
  let bits = 16;
  let dataAt = -1;
  let dataLen = 0;
  while (pos + 8 <= buf.length) {
    const id = buf.toString('ascii', pos, pos + 4);
    const len = buf.readUInt32LE(pos + 4);
    if (id === 'fmt ') {
      channels = buf.readUInt16LE(pos + 10);
      rate = buf.readUInt32LE(pos + 12);
      bits = buf.readUInt16LE(pos + 22);
    } else if (id === 'data') {
      dataAt = pos + 8;
      dataLen = Math.min(len, buf.length - dataAt);
      break;
    }
    pos += 8 + len + (len % 2);
  }
  if (dataAt < 0 || bits !== 16) return null;
  const frameBytes = 2 * channels;
  const total = Math.floor(dataLen / frameBytes);
  const win = Math.max(1, Math.floor(rate / 50));
  let first = -1;
  let last = -1;
  for (let i = 0; i + win <= total; i += win) {
    let acc = 0;
    for (let k = 0; k < win; k++) {
      const v = buf.readInt16LE(dataAt + (i + k) * frameBytes);
      acc += v * v;
    }
    if (Math.sqrt(acc / win) > 600) {
      if (first < 0) first = i;
      last = i + win;
    }
  }
  return { duration: total / rate, speech: first < 0 ? null : [first / rate, last / rate] };
}

// ---- Part 2: nothing loads early ---------------------------------------------------------------------------------

interface Seen {
  url: string;
  origin: string;
  stage: string;
  at: number;
  bytes?: number;
  decoded?: number;
  /** True when the request was made by the page's service worker (the worker's own fetch of a request the page made). */
  sw?: boolean;
}

async function part2(browser: Browser, w: World) {
  const ctx = await newCtx(browser, { theme: 'lounge-light' });
  const voice = await voiceChannelOf(ctx, w);
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  let stage = 'start';
  const t0 = Date.now();
  const all: Seen[] = [];
  const csp: string[] = [];
  const sizes: Promise<void>[] = [];
  ctx.on('request', (req: Request) => {
    const u = req.url();
    if (u.startsWith('data:') || u.startsWith('blob:')) return;
    const seen: Seen = {
      url: u,
      origin: new URL(u).origin,
      stage,
      at: Date.now() - t0,
      sw: !!req.serviceWorker(),
    };
    all.push(seen);
    if (isVadFile(u))
      sizes.push(
        (async () => {
          const res = await req.response().catch(() => null);
          if (!res) return;
          const body = await res.body().catch(() => null);
          const sz = await req.sizes().catch(() => null);
          seen.decoded = body ? body.length : undefined;
          seen.bytes = sz ? sz.responseBodySize : undefined;
        })(),
      );
  });
  page.on('console', (m) => {
    if (/Content Security Policy|Refused to (load|connect|create|execute)/i.test(m.text()))
      csp.push(m.text().slice(0, 200));
  });
  try {
    const run = async (name: string, fn: () => Promise<void>) => {
      stage = name;
      await fn();
      await page.waitForTimeout(1200);
    };
    await run('lounge', async () => {
      await page.goto(`/channels/${w.guildId}/${w.channelId}`, { waitUntil: 'domcontentloaded' });
      await page.getByRole('log').waitFor({ timeout: 30_000 });
      await settle(page);
    });
    await run('voice-settings', async () => {
      await openVoiceSettings(page);
    });
    await run('mic-test-standard', async () => {
      await page.getByRole('button', { name: "Let's check" }).click();
      await page.waitForTimeout(3000);
      await page.getByRole('button', { name: 'Stop testing' }).click();
    });
    await run('call-standard', async () => {
      await joinCall(page, w, voice.id);
      await page.waitForTimeout(5000);
      await leaveCall(page);
    });
    const early = all.filter((s) => isVadFile(s.url));
    figures.earlyRequests = early;
    if (early.length)
      problem(
        `nothing-loads-early: ${early.length} request(s) for the detector's files with Standard: ${early.map((e) => `${e.stage}: ${e.url}`).join(' | ')}`,
      );
    else
      say(
        `nothing-loads-early: no request names silero_vad, ort-wasm or the runtime chunk from the Lounge to a joined call (${all.length} requests)`,
      );

    await run('choose-silero', async () => {
      await openVoiceSettings(page);
      await chooseEngine(page, 'silero');
    });
    const afterChoose = all.filter((s) => isVadFile(s.url) && s.stage === 'choose-silero');
    if (afterChoose.length)
      problem(
        `choosing Silero alone already requested: ${afterChoose.map((e) => e.url).join(' | ')} (the contract says: at a call or the microphone test)`,
      );
    await run('mic-test-silero', async () => {
      await page.getByRole('button', { name: "Let's check" }).click();
      await statusLine(page)
        .first()
        .waitFor({ timeout: 10_000 })
        .catch(() => undefined);
      await page
        .waitForFunction(
          () =>
            ['ready', 'fallback'].includes(
              document
                .querySelector('[data-testid="vad-engine-status"]')
                ?.getAttribute('data-state') ?? '',
            ),
          undefined,
          { timeout: 60_000 },
        )
        .catch(() => undefined);
      await page.waitForTimeout(2500);
    });
    const state = await statusLine(page)
      .first()
      .getAttribute('data-state')
      .catch(() => null);
    figures.statusAfterMicTest = state;
    if (state !== 'ready') problem(`mic test with Silero: status is "${state}", not ready`);
    await Promise.all(sizes);
    const files = all.filter((s) => isVadFile(s.url) && !s.sw);
    figures.serviceWorkerCopies = all.filter((s) => isVadFile(s.url) && s.sw);
    const table: Record<string, Seen[]> = { model: [], wasm: [], runtime: [] };
    for (const f of files) {
      const path = f.url.split('?')[0] ?? f.url;
      if (MODEL_RE.test(path)) table.model?.push(f);
      else if (WASM_RE.test(path)) table.wasm?.push(f);
      else table.runtime?.push(f);
    }
    figures.files = table;
    for (const [kind, list] of Object.entries(table)) {
      if (list.length !== 1)
        problem(
          `${kind}: requested ${list.length} time(s), expected once (${list.map((x) => `${x.stage} ${x.url}`).join(' | ')})`,
        );
      for (const f of list)
        if (f.origin !== ORIGIN)
          problem(`${kind}: requested from ${f.origin}, not the page's own origin ${ORIGIN}`);
    }
    const foreign = all.filter((s) => s.origin !== ORIGIN);
    figures.foreignRequests = foreign.map((f) => `${f.stage}: ${f.url}`);
    if (foreign.length)
      problem(`requests to another origin: ${foreign.map((f) => f.url).join(' | ')}`);
    const pageCsp = await page.evaluate(
      () => (window as unknown as { __cspViolations?: string[] }).__cspViolations ?? [],
    );
    figures.cspViolations = { page: pageCsp, console: csp };
    if (pageCsp.length || csp.length)
      problem(`CSP violations: ${JSON.stringify({ pageCsp, csp })}`);
    figures.requestCount = all.length;
    for (const [kind, list] of Object.entries(table))
      for (const f of list)
        say(
          `${kind}: ${f.url} first requested in "${f.stage}" at ${f.at} ms, ${f.bytes ?? '?'} B transferred, ${f.decoded ?? '?'} B decoded`,
        );
  } finally {
    if (voice.made) await apiDelete(ctx.request, `/channels/${voice.id}`).catch(() => undefined);
    await ctx.close();
  }
}

// ---- Part 3: the detector on a fake microphone -------------------------------------------------------------------

interface Row {
  input: string;
  engine: string;
  state: string | null;
  frames: number;
  speechFrames: number;
  ratio: number | null;
  speakingFraction: number;
  meanMs: number | null;
  maxMs: number | null;
  frameMs: number;
  onIntervals: { from: number; to: number }[];
  heapBefore?: number | null;
  heapAfter?: number | null;
}

async function callRun(
  browser: Browser,
  w: World,
  voiceId: string,
  input: string,
  engine: 'energy' | 'silero',
  seconds: number,
): Promise<Row> {
  const ctx = await newCtx(browser, { voice: BEEP_VOICE });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  try {
    await openVoiceSettings(page);
    const heapBefore = await heap(page);
    if (engine === 'silero') await chooseEngine(page, 'silero');
    await joinCall(page, w, voiceId);
    const settled = await waitSettled(page, engine);
    if (engine === 'silero' && settled?.state !== 'ready')
      problem(
        `${input}/${engine}: the detector is "${settled?.state}" (${settled?.fallbackReason ?? 'no reason'}), not ready`,
      );
    if (!settled)
      problem(
        `${input}/${engine}: window.__srVadStats never appeared (is the bundle built with VITE_SR_E2E=1?)`,
      );
    await page.waitForTimeout(1000);
    const sm = await sample(page, seconds);
    const d = delta(sm);
    const e = sm.statsEnd;
    const row: Row = {
      input,
      engine: e?.engine ?? '?',
      state: e?.state ?? null,
      frames: d.frames,
      speechFrames: d.speechFrames,
      ratio: d.ratio,
      speakingFraction: round(sm.onFraction * 1000) / 1000,
      meanMs: e ? e.meanMs : null,
      maxMs: e ? e.maxMs : null,
      frameMs: e?.frameMs ?? 32,
      onIntervals: sm.onIntervals.slice(0, 40),
      heapBefore,
      heapAfter: await heap(page),
    };
    await leaveCall(page);
    return row;
  } finally {
    await ctx.close();
  }
}

async function part3(browser: Browser, w: World) {
  const ownerCtx = await newCtx(browser);
  const voice = await voiceChannelOf(ownerCtx, w);
  const rows: Row[] = [];
  const tmp = mkdtempSync(resolve(tmpdir(), 'p115vad-'));
  let own: Browser | null = null;
  try {
    // The default fake device: Chromium's beep.
    const beepEnergy = await callRun(browser, w, voice.id, 'beep', 'energy', 20);
    rows.push(beepEnergy);
    if (beepEnergy.engine !== 'energy') problem(`beep/energy: engine reads "${beepEnergy.engine}"`);
    if (beepEnergy.speakingFraction <= 0)
      problem(
        'beep/energy: the speaking state never turned on (the energy gate should open on the beeps)',
      );
    const beepSilero = await callRun(browser, w, voice.id, 'beep', 'silero', 20);
    rows.push(beepSilero);
    if (beepSilero.state === 'ready') {
      if (beepSilero.speakingFraction > 0)
        problem(
          `beep/silero: the speaking state was on for ${round(beepSilero.speakingFraction * 100)} % of the samples (${JSON.stringify(beepSilero.onIntervals.slice(0, 5))}); must never turn on over the 20 seconds`,
        );
    }
    // The cost.
    for (const r of rows.filter((x) => x.engine === 'silero')) {
      if (r.meanMs !== null && r.meanMs > r.frameMs / 4)
        problem(
          `cost: ${r.input}/silero mean ${r.meanMs} ms per frame is over a quarter of the ${r.frameMs} ms frame`,
        );
      if (r.maxMs !== null && r.maxMs > r.frameMs)
        problem(
          `cost: ${r.input}/silero worst frame ${r.maxMs} ms is longer than the frame (${r.frameMs} ms)`,
        );
    }

    // Own Chromium for the file inputs (as keyboard-6-voice launches its own arguments).
    const silencePath = resolve(tmp, 'silence-16k.wav');
    writeFileSync(silencePath, silenceWav(30));
    const files: { name: string; path: string }[] = [{ name: 'silence', path: silencePath }];
    const speechOk = existsSync(SPEECH);
    if (speechOk) files.push({ name: 'speech', path: SPEECH });
    else note('not proven: no speech fixture (e2e/fixtures/vad/speech-16k.wav is absent)');
    for (const f of files) {
      own = await chromium.launch({
        args: [...CHROMIUM_ARGS, `--use-file-for-fake-audio-capture=${f.path}`],
      });
      try {
        const seconds = f.name === 'speech' ? 24 : 20;
        for (const engine of ['energy', 'silero'] as const) {
          const r = await callRun(own, w, voice.id, f.name, engine, seconds);
          rows.push(r);
          if (f.name === 'silence') {
            if (r.speechFrames > 0)
              problem(`silence/${engine}: ${r.speechFrames} speech frames on silence`);
            if (r.speakingFraction > 0)
              problem(`silence/${engine}: the speaking state turned on on silence`);
          }
        }
      } finally {
        await own.close().catch(() => undefined);
        own = null;
      }
    }
    if (speechOk) {
      const wf = wavFigures(readFileSync(SPEECH));
      figures.speechFile = wf;
      const r = rows.find((x) => x.input === 'speech' && x.engine === 'silero');
      if (r && wf?.speech) {
        const speechLen = wf.speech[1] - wf.speech[0];
        if (!r.onIntervals.length) problem('speech/silero: the speaking state never turned on');
        for (const iv of r.onIntervals) {
          const len = iv.to - iv.from;
          if (len > speechLen + 1.5)
            problem(
              `speech/silero: speaking stayed on ${len} s for a ${round(speechLen)} s speech (more than 1 s after it ends: ${JSON.stringify(iv)})`,
            );
        }
        const offTime = (r.speakingFraction === 1 ? 0 : 1 - r.speakingFraction) * 24;
        if (offTime < 1)
          problem('speech/silero: the speaking state never turned off between the speeches');
      } else if (r && !wf)
        note(
          'speech/silero: the fixture is not a 16-bit PCM WAV this spec can read; timing not judged',
        );
    }
    figures.rows = rows;
    // The page's JS heap: settings page (Standard) before, then in each call after 20 s.
    figures.heapNote =
      'usedJSHeapSize of the page (the worker has its own heap and is not in it): heapBefore = the settings page before the call, heapAfter = in the call after the sample';
    for (const r of rows)
      say(
        `${r.input} / ${r.engine} (${r.state}): frames ${r.frames}, speech ${r.speechFrames}, speaking ${round(r.speakingFraction * 100)} %, mean ${r.meanMs} ms, max ${r.maxMs} ms (frame ${r.frameMs} ms), heap ${r.heapBefore} -> ${r.heapAfter}`,
      );
  } finally {
    if (own) await (own as Browser).close().catch(() => undefined);
    rmSync(tmp, { recursive: true, force: true });
    if (voice.made)
      await apiDelete(ownerCtx.request, `/channels/${voice.id}`).catch(() => undefined);
    await ownerCtx.close();
  }
}

// ---- Part 4: falling back ----------------------------------------------------------------------------------------

async function part4(browser: Browser, w: World) {
  const ctx = await newCtx(browser, { voice: BEEP_VOICE, blockSw: true });
  const voice = await voiceChannelOf(ctx, w);
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  const out: Record<string, unknown> = {};
  const notice = (p: Page) =>
    p
      .locator('[role=status],[role=alert]')
      .filter({ hasText: /Silero voice detector could not start/i });
  try {
    // The loading state: the model is delayed, the microphone test shows `loading`.
    await openVoiceSettings(page);
    await chooseEngine(page, 'silero');
    await ctx.route(MODEL_PATTERN, async (route) => {
      await new Promise((r) => setTimeout(r, 6000));
      await route.continue().catch(() => undefined);
    });
    await page.getByRole('button', { name: "Let's check" }).click();
    const loading = await page
      .waitForFunction(
        () =>
          document
            .querySelector('[data-testid="vad-engine-status"]')
            ?.getAttribute('data-state') === 'loading',
        undefined,
        { timeout: 8000 },
      )
      .then(() => true)
      .catch(() => false);
    out.loadingCaught = loading;
    if (loading) await shot(page, 'voice-mode-silero-loading-1280-light');
    else note('the loading state was not caught with a model delayed by 6 s');
    await page
      .getByRole('button', { name: 'Stop testing' })
      .click()
      .catch(() => undefined);
    await ctx.unroute(MODEL_PATTERN);

    const variants: {
      key: string;
      pattern: string | RegExp;
      handler: (r: import('@playwright/test').Route) => Promise<void>;
    }[] = [
      {
        key: 'model-404',
        pattern: MODEL_PATTERN,
        handler: (r) => r.fulfill({ status: 404, body: 'not found' }),
      },
      { key: 'wasm-aborted', pattern: WASM_PATTERN, handler: (r) => r.abort('failed') },
    ];
    for (const v of variants) {
      const res: Record<string, unknown> = {};
      await ctx.route(v.pattern, v.handler);
      try {
        await openVoiceSettings(page);
        await chooseEngine(page, 'silero');
        // The settings status after a microphone test that cannot load.
        await page.getByRole('button', { name: "Let's check" }).click();
        await page
          .waitForFunction(
            () =>
              document
                .querySelector('[data-testid="vad-engine-status"]')
                ?.getAttribute('data-state') === 'fallback',
            undefined,
            { timeout: 30_000 },
          )
          .then(() => (res.settingsFallback = true))
          .catch(() => (res.settingsFallback = false));
        if (v.key === 'model-404' && res.settingsFallback)
          await shot(page, 'voice-mode-silero-fallback-1280-light');
        if (!res.settingsFallback)
          problem(
            `${v.key}: the settings status never reached fallback during the microphone test`,
          );
        await page
          .getByRole('button', { name: 'Stop testing' })
          .click()
          .catch(() => undefined);

        // In a call.
        await joinCall(page, w, voice.id);
        const s = await waitSettled(page, 'silero', 40_000);
        res.stats = s;
        if (s?.state !== 'fallback')
          problem(`${v.key}: in the call the state is "${s?.state}", not fallback`);
        if (!s?.fallbackReason) problem(`${v.key}: __srVadStats.fallbackReason is not set`);
        await page.waitForTimeout(1500);
        const count = await notice(page).count();
        res.noticeCount = count;
        if (count !== 1) problem(`${v.key}: ${count} fallback notice(s) shown, expected 1`);
        if (v.key === 'model-404') await shot(page, 'call-fallback-notice-1280-light');
        const sm = await sample(page, 8);
        res.speakingFraction = round(sm.onFraction * 1000) / 1000;
        res.onIntervals = sm.onIntervals.slice(0, 20);
        if (sm.onFraction <= 0)
          problem(
            `${v.key}: after the fallback the speaking state never turned on (the energy gate should take over on the beeps)`,
          );
        const after = await readStats(page);
        res.statsAfter = after;
        if (after && after.state !== 'fallback')
          problem(`${v.key}: the state changed to "${after.state}" after the fallback`);
        await leaveCall(page);
      } catch (e) {
        problem(`${v.key}: failed (${String(e).split('\n')[0]?.slice(0, 200)})`);
      } finally {
        await ctx.unroute(v.pattern).catch(() => undefined);
      }
      out[v.key] = res;
    }

    // Un-routed: leave, join again, Silero loads (the setting was kept).
    const again: Record<string, unknown> = {};
    try {
      await joinCall(page, w, voice.id);
      const s = await waitSettled(page, 'silero', 60_000);
      again.stats = s;
      if (s?.state !== 'ready')
        problem(
          `rejoin without routes: state "${s?.state}" (${s?.fallbackReason ?? 'no reason'}), not ready: the setting or the retry was lost`,
        );
      await leaveCall(page);
    } catch (e) {
      problem(`rejoin: failed (${String(e).split('\n')[0]?.slice(0, 200)})`);
    }
    out.rejoin = again;
    figures.fallback = out;
  } finally {
    if (voice.made) await apiDelete(ctx.request, `/channels/${voice.id}`).catch(() => undefined);
    await ctx.close();
  }
}

// ---- Part 5: E2EE ------------------------------------------------------------------------------------------------

async function e2eeSnapshot(page: Page) {
  return page.evaluate(() => {
    const get = (globalThis as unknown as { __srE2ee?: () => Record<string, unknown> }).__srE2ee;
    const v = get?.() as
      | {
          active: boolean;
          call: { status: string; callCode: string | null; members: unknown[] } | null;
          workerError?: string | null;
        }
      | undefined;
    const panel = document.querySelector('[aria-label="Voice connection"]');
    return {
      available: !!get,
      active: v?.active ?? false,
      status: v?.call?.status ?? null,
      hasCallCode: !!v?.call?.callCode,
      members: v?.call?.members.length ?? 0,
      workerError: v?.workerError ?? null,
      panelText: (panel?.textContent ?? '')
        .replace(/\d+:\d+/g, '')
        .replace(/\s+/g, ' ')
        .trim()
        .slice(0, 200),
    };
  });
}

async function part5(browser: Browser, w: World) {
  const probe = await newCtx(browser);
  const voice = await voiceChannelOf(probe, w);
  const snaps: Record<string, unknown> = {};
  try {
    for (const engine of ['energy', 'silero'] as const) {
      const ctx = await newCtx(browser, { voice: BEEP_VOICE });
      const page = await ctx.newPage();
      page.setDefaultTimeout(20_000);
      try {
        await openVoiceSettings(page);
        if (engine === 'silero') await chooseEngine(page, 'silero');
        await joinCall(page, w, voice.id);
        const s = await waitSettled(page, engine);
        if (engine === 'silero' && s?.state !== 'ready')
          problem(`e2ee/silero: state "${s?.state}", not ready`);
        await page.waitForTimeout(4000);
        const sm = await sample(page, 6);
        snaps[engine] = {
          e2ee: await e2eeSnapshot(page),
          speakingFraction: round(sm.onFraction * 1000) / 1000,
          stats: s,
        };
        if (sm.onFraction <= 0 && engine === 'energy')
          problem('e2ee/energy: the speaking state never turned on');
        await leaveCall(page);
      } finally {
        await ctx.close();
      }
    }
    figures.e2ee = snaps;
    const a = (snaps.energy as { e2ee?: unknown } | undefined)?.e2ee;
    const b = (snaps.silero as { e2ee?: unknown } | undefined)?.e2ee;
    if (JSON.stringify(a) !== JSON.stringify(b))
      problem(
        `e2ee: the call reads differently with Silero: standard ${JSON.stringify(a)} vs silero ${JSON.stringify(b)}`,
      );
    const sil = snaps.silero as
      { speakingFraction?: number; e2ee?: { active?: boolean; available?: boolean } } | undefined;
    if (sil?.e2ee?.available === false)
      note(
        'e2ee: window.__srE2ee is not present in this bundle: only the connection panel text was compared',
      );
    if (sil && (sil.speakingFraction ?? 0) > 0)
      note(
        'e2ee: the speaking state turned on with Silero on the beeps (the other half of the beep result)',
      );
    if (!sil || !(sil.speakingFraction !== undefined)) problem('e2ee: no silero snapshot');
  } finally {
    if (voice.made) await apiDelete(probe.request, `/channels/${voice.id}`).catch(() => undefined);
    await probe.close();
  }
}

// ---- The test ----------------------------------------------------------------------------------------------------

test(`wave 115 vadshots: the Silero detector, ${PHASE} part ${PART}`, async ({
  browser,
  browserName,
}) => {
  test.skip(
    browserName !== 'chromium',
    'Fake media flags and the detector checks are Chromium only.',
  );
  mkdirSync(OUT, { recursive: true });
  const w = readWorld();
  try {
    if (PART === '1') await part1(browser);
    else if (PHASE === 'base')
      note(`part ${PART} is an after-phase part: nothing to do in the base phase`);
    else if (PART === '2') await part2(browser, w);
    else if (PART === '3') await part3(browser, w);
    else if (PART === '4') await part4(browser, w);
    else if (PART === '5') await part5(browser, w);
  } finally {
    const file = PART === '1' ? `figures-${PHASE}.json` : `figures-${PHASE}-part${PART}.json`;
    writeFileSync(
      resolve(OUT, file),
      JSON.stringify({ phase: PHASE, part: PART, figures, problems, notes }, null, 1),
    );
    say(`part ${PART} (${PHASE}): ${problems.length} problem line(s), ${notes.length} note(s)`);
    for (const p of problems) say(`  PROBLEM ${p}`);
  }
});
