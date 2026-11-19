import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPost, test } from '../support/fixtures';

/**
 * Wave 108, seed (D-1026, SEC-AUD-529): what the sweep world now holds beside the old data, opened at 375 x 812 touch
 * (Lounge Light) the way a member opens it, one shot each, each asserted present by the id recorded in `world.json`
 * under the key `extras`. No hit test: measuring is the second census's.
 *
 * The world is made FRESH by the job (`--fresh`: an empty database, "seed the sweep world" of THIS tree, then this
 * spec), so there is no base twin of the world: the old world is proved intact against what the base's seed writes
 * (the same world.json keys, the second text channel and the voice channel still there, 11 messages in the seeded
 * channel). The five additions are asserted by the ids in `extras`, and shot.
 *
 * Switches: SR_P108_SEED_ON=1, SR_P108_SEED_PHASE=base|after (only the shots' name prefix), SR_P108_SEED_OUT=<shots folder). Figures: figures-<phase>.json.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_SEED_ON=1 --env SR_P108_SEED_PHASE=after --env SR_P108_SEED_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-seed -- tests/p108-seed-shots.spec.ts
 * (the seed's own jobs carry --fresh; a spec of another builder never asserts on `extras`).
 */

const ON = process.env.SR_P108_SEED_ON === '1';
const PHASE = process.env.SR_P108_SEED_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P108_SEED_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-seed';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const problems: string[] = [];
const figures: Record<string, unknown> = {};

interface Extras {
  friendId: string | null;
  friendName: string | null;
  askerId: string | null;
  askerName: string | null;
  groupDmId: string | null;
  groupDmName: string | null;
  extrasChannelId: string | null;
  extrasChannelName: string | null;
  reactionsMessageId: string | null;
  spoilerMessageId: string | null;
  spoilerCommentId: string | null;
  spoilerCommentTitleId: string | null;
  listId: string | null;
  listName: string | null;
  notSeeded: string[];
}
type World = Record<string, unknown> & {
  guildId: string;
  channelId: string;
  titleId: string;
  seedLog: string[];
  extras?: Extras;
};
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

async function phonePage(browser: Browser, owner: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  await ctx.addInitScript(() => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: 'lounge-light', zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function settle(page: Page) {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(800);
}
async function go(page: Page, url: string) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await settle(page);
}
async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sw}/${cw}`);
  if (sw > cw) problems.push(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  await page.mouse.move(1, 1).catch(() => undefined);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}
/** One thing: run `fn`; a throw is a problem line (never a fail of the spec: the lines are the output). */
async function check(name: string, fn: () => Promise<void>) {
  try {
    await fn();
    say(`ok ${name}`);
  } catch (e) {
    problems.push(`${name}: ${String(e).replace(/\s+/g, ' ').slice(0, 220)}`);
  }
}
async function seen(page: Page, text: string | RegExp, what: string) {
  await page.getByText(text).first().waitFor({ timeout: 15_000 });
  say(`seen ${what}`);
}

test(`wave 108 seed: the sweep world's additions, ${PHASE}`, async ({ browser }) => {
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = owner.request;
  const apiGet = async <T>(path: string) => {
    const r = await o.get(`/api/v1${path}`);
    if (!r.ok()) throw new Error(`GET ${path} -> ${r.status()}`);
    return (await r.json()) as T;
  };

  // The seed's own log: every FAIL is a problem line.
  for (const l of w.seedLog) if (l.startsWith('FAIL')) problems.push(`seed log: ${l}`);
  figures.seedLogLines = w.seedLog.length;

  // The old world: its keys, the channels of the guild, the messages of the seeded channel.
  const oldKeys = Object.keys(w).filter((k) => k !== 'extras' && k !== 'seedLog');
  const channels = await apiGet<{ id: string; type: number; name: string }[]>(
    `/guilds/${w.guildId}/channels`,
  );
  const msgs = await apiGet<{ id: string }[]>(`/channels/${w.channelId}/messages?limit=100`);
  figures.oldKeys = oldKeys;
  figures.channels = channels.length;
  figures.seededChannelMessages = msgs.length;
  figures.seedLogOk = w.seedLog.filter((l) => l.startsWith('ok')).length;

  const x = w.extras;
  if (!x) {
    problems.push('world.json has no `extras` key');
  } else {
    figures.extras = x;
    const { ctx, page } = await phonePage(browser, owner);
    try {
      // (1) The Friends page with a friend (All) and a request (Pending).
      await check('friends: a friend and a request', async () => {
        if (!x.friendId || !x.askerId) throw new Error('no friend or requester id recorded');
        const rel = await apiGet<{ id?: string; type: number; user?: { id: string } }[]>(
          '/users/@me/relationships',
        );
        const has = (id: string, type: number) =>
          rel.some((r) => (r.user?.id ?? r.id) === id && r.type === type);
        if (!has(x.friendId, 1)) throw new Error('the friend is not type 1 in /relationships');
        if (!has(x.askerId, 3)) throw new Error('the request is not type 3 (incoming)');
        await go(page, '/channels/@me');
        await page
          .getByRole('button', { name: /^Friends/ })
          .or(page.getByRole('link', { name: /^Friends/ }))
          .first()
          .click();
        await page.locator('section.lng-friends').first().waitFor({ timeout: 10_000 });
        await page
          .getByRole('tab', { name: /^All/ })
          .or(page.getByRole('button', { name: /^All$/ }))
          .first()
          .click();
        await page
          .getByRole('button', { name: `Message ${x.friendName}` })
          .first()
          .waitFor();
        await shot(page, 'friends-all');
        await page
          .getByRole('tab', { name: /^Pending/ })
          .or(page.getByRole('button', { name: /^Pending/ }))
          .first()
          .click();
        await page
          .getByRole('button', { name: `Accept ${x.askerName}` })
          .first()
          .waitFor();
        await shot(page, 'friends-pending');
      });

      // (2) The group DM.
      await check('group dm', async () => {
        if (!x.groupDmId) throw new Error('no group DM id recorded');
        const all =
          await apiGet<{ id: string; type: number; recipients?: unknown[] }[]>(
            '/users/@me/channels',
          );
        const ch = all.find((c) => c.id === x.groupDmId);
        if (!ch) throw new Error('the group DM is not in /users/@me/channels');
        figures.groupDmType = ch.type;
        figures.groupDmRecipients = ch.recipients?.length ?? null;
        if (ch.type !== 3) throw new Error(`channel type ${ch.type}, wanted 3 (group DM)`);
        await go(page, `/channels/@me/${x.groupDmId}`);
        await seen(page, 'Count me in.', 'the last group DM message');
        await shot(page, 'group-dm');
      });

      // (3) The reactions line (wraps) and the spoiler message, in the new channel.
      await check('reactions that wrap', async () => {
        if (!x.extrasChannelId || !x.reactionsMessageId)
          throw new Error('no channel or message id');
        await go(page, `/channels/${w.guildId}/${x.extrasChannelId}`);
        await seen(page, 'Which snack should we get?', 'the reactions message');
        const rows = await page
          .locator('.lng-reactions')
          .first()
          .evaluate((el) => {
            const tops = new Set<number>();
            for (const b of Array.from(el.querySelectorAll('button')))
              tops.add(Math.round(b.getBoundingClientRect().top));
            return { pills: el.querySelectorAll('button').length, lines: tops.size };
          });
        figures.reactionPills = rows.pills;
        figures.reactionLines = rows.lines;
        if (rows.pills < 14) throw new Error(`${rows.pills} reaction pills, wanted 14`);
        if (rows.lines < 2) throw new Error(`the reactions fit on ${rows.lines} line at 375`);
        await shot(page, 'reactions-wrapped');
      });
      await check('spoiler message', async () => {
        if (!x.spoilerMessageId) throw new Error('no spoiler message id recorded');
        await seen(page, 'The ending of the film', 'the spoiler message');
        await page.locator('.sr-md-spoiler').first().scrollIntoViewIfNeeded();
        await shot(page, 'spoiler-message');
      });

      // Attachments: probed, not seeded (an upload slot, then a message that names it).
      await check('attachments probe', async () => {
        if (!x.extrasChannelId) throw new Error('no channel id');
        const slot = await apiPost<{
          attachments: { upload_id: string; upload_filename: string }[];
        }>(o, `/channels/${x.extrasChannelId}/attachments`, {
          files: [{ filename: 'probe.txt', file_size: 5, id: '1' }],
        });
        try {
          await apiPost(o, `/channels/${x.extrasChannelId}/messages`, {
            content: 'p108 seed probe',
            attachments: [
              {
                id: '1',
                filename: 'probe.txt',
                uploaded_filename: slot.attachments[0]?.upload_filename,
              },
            ],
          });
          figures.attachmentProbe = 'a message naming an unuploaded file was ACCEPTED';
        } catch (e) {
          figures.attachmentProbe = `refused: ${String(e).replace(/\s+/g, ' ').slice(0, 160)}`;
        }
        say(`attachment probe: ${String(figures.attachmentProbe)}`);
      });

      // (4) The Cinema comment with a spoiler.
      await check('cinema comment with a spoiler', async () => {
        if (!x.spoilerCommentId || !x.spoilerCommentTitleId) throw new Error('no comment id');
        const list = await apiGet<{ items?: { id: string; spoiler: boolean }[] }>(
          `/cinema/titles/${x.spoilerCommentTitleId}/comments`,
        );
        const hit = list.items?.find((c) => c.id === x.spoilerCommentId);
        if (!hit) throw new Error('the comment is not in the title comments list');
        if (!hit.spoiler) throw new Error('the comment is listed without the spoiler flag');
        await go(page, `/cinema/t/${x.spoilerCommentTitleId}`);
        await page
          .getByRole('tab', { name: /^Comments/ })
          .first()
          .click();
        await seen(page, /This contains spoilers/, 'the spoiler note on the comment');
        await shot(page, 'cinema-comment-spoiler');
      });

      // (5) The Library list.
      await check('library list', async () => {
        if (!x.listId || !x.listName) throw new Error('no list id recorded');
        await go(page, `/library/lists/${x.listId}`);
        await seen(page, x.listName, 'the list name');
        await shot(page, 'library-list');
      });
    } finally {
      await ctx.close();
    }
  }

  // The old world is intact (D-1026): the keys the base's seed wrote, the channels it made, the seeded channel's messages
  // (the base seed posts 8 messages and the a11y world 3 more: 11; the new messages live in the new channel).
  const BASE_KEYS = [
    'guildId',
    'channelId',
    'voiceChannelId',
    'messageId',
    'threadId',
    'titleId',
    'partyId',
    'dmId',
    'appId',
    'activityAppId',
    'tvChannelId',
    'inviteCode',
    'memberName',
  ];
  const missing = BASE_KEYS.filter((k) => !(k in w));
  if (missing.length) problems.push(`world.json lost keys: ${missing.join(', ')}`);
  const extraKeys = oldKeys.filter((k) => !BASE_KEYS.includes(k));
  if (extraKeys.length) problems.push(`world.json has unexpected keys: ${extraKeys.join(', ')}`);
  const names = channels.map((c) => c.name);
  if (!names.includes('announcements-and-very-long-channel-name'))
    problems.push('the second text channel is missing');
  if (!channels.some((c) => c.id === w.voiceChannelId))
    problems.push('the voice channel is missing');
  figures.channelNames = names;
  // The base seed leaves 11 posted messages (8 + the a11y world's 3) plus the server's own join notices; none of ours is here.
  const ours = await apiGet<{ content: string }[]>(`/channels/${w.channelId}/messages?limit=100`);
  if (ours.length < 11) problems.push(`seeded channel messages: ${ours.length}, fewer than 11`);
  if (ours.some((m) => /Which snack|ending of the film|Sweep X/.test(m.content)))
    problems.push('a message of the extras landed in the seeded channel');
  figures.problems = problems;
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figures, null, 1));
  for (const p of problems) say(`PROBLEM ${p}`);
  await owner.close();
});
