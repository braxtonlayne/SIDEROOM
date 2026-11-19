import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import type { APIRequestContext } from '@playwright/test';
import { DATABASE_URL } from './cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser } from './fixtures';
import { apiDelete } from './world';

/**
 * Wave 100 (D-952): two test rows that no world holds, made by a named function a spec calls. `world.ts` is not
 * changed; every name carries `p100-seed-` and a random suffix (D-942), so a second call (or another spec) never
 * picks the other's rows. Each function is safe to call twice in one world: it adds new rows, it never reuses one.
 *
 * What they need from the stack:
 *  - `seedTranscodingRow`: the Postgres of the stack (`DATABASE_URL`, or the one passed in `ctx`) and `psql` (on PATH,
 *    or `PSQL`, or the default Windows install). Nothing else: no transcoder, no S3, no ffmpeg.
 *    WHY SQL: the only product routes that queue a job need a stored video. A Cinema upload and a mount conversion
 *    both read an object from the bucket (`begin_upload` presigns a part, `POST /storage/mounts/{id}/transcode` heads
 *    the key), and on the stack's `SR_STORAGE__ENDPOINT=memory:` no byte can be put there (presigned PUTs are fake
 *    `memory://` URLs). So the row is inserted into `transcode_jobs` in the stack's own scratch database, as
 *    `jukebox.spec.ts` and `arcade-sandbox.spec.ts` seed theirs. It is the table the Admin list reads (`GET
 *    /admin/transcode/jobs`); the row is a `mount_transcode` job of the instance mount (id 1), queued or failed.
 *  - `seedActivityTier`: the Owner's session (`ctx.owner`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`. It adds an
 *    Arcade "Coming soon" game through the Operator's route (`POST /admin/arcade/coming-soon`), the one place an
 *    Operator sets a tier without a bundle; members see the tier on the Arcade page, staff on Admin > Arcade.
 */

export interface SeedCtx {
  /** The Owner's (Operator's) request context: needed by `seedActivityTier` only. */
  owner?: APIRequestContext;
  /** The stack's Postgres URL; default `DATABASE_URL` of `cli.ts`. Never printed. */
  databaseUrl?: string;
}

const PREFIX = 'p100-seed-';
const suffix = () => Math.random().toString(36).slice(2, 8);

function psqlPath(): string {
  if (process.env.PSQL) return process.env.PSQL;
  const win = 'C:/Program Files/PostgreSQL/17/bin/psql.exe';
  return existsSync(win) ? win : 'psql';
}

/** Run one statement (`-c`, stdin closed); the password goes by PGPASSWORD, never on the command line. */
function psql(databaseUrl: string, sql: string): string {
  const u = new URL(databaseUrl);
  return execFileSync(
    psqlPath(),
    [
      '-X',
      '-A',
      '-t',
      '-q',
      '-h',
      u.hostname,
      '-p',
      u.port || '5432',
      '-U',
      decodeURIComponent(u.username),
      '-d',
      u.pathname.slice(1),
      '-v',
      'ON_ERROR_STOP=1',
      '-c',
      sql,
    ],
    {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, PGPASSWORD: decodeURIComponent(u.password) },
    },
  ).trim();
}

/** A snowflake-shaped id (ms since 2023-11 in the high bits, 22 random bits below): unique per call. */
function jobId(): string {
  const ms = BigInt(Date.now() - 1_700_000_000_000);
  const low = BigInt(Math.floor(Math.random() * 4_194_303));
  return String((ms << 22n) | low);
}

export interface SeededJob {
  id: string;
  state: 'queued' | 'running' | 'failed';
  /** The source key the job names (the Admin table does not show it; it is in `source_key`). */
  key: string;
  /** The text the Admin row shows that carries the seeded name (the `error` of a failed job), or null. */
  text: string | null;
}
export interface SeededTranscoding {
  /** The random part of every name made by this call. */
  suffix: string;
  jobs: SeededJob[];
  /** Delete exactly these rows again. */
  remove: () => void;
}

/**
 * Three rows in Admin > Transcoding > Queue: a queued job, a running one with a stage and a percentage, and a failed
 * one whose error is a long text with an unbroken file name (the worst case for a cell's width).
 */
export function seedTranscodingRow(ctx: SeedCtx = {}): SeededTranscoding {
  const db = ctx.databaseUrl ?? DATABASE_URL;
  const sfx = suffix();
  const name = `${PREFIX}${sfx}`;
  const spec = (key: string) =>
    JSON.stringify({
      kind: 'mount_transcode',
      mount_id: 1,
      key,
      format: 'hls',
      preset: 'standard',
      operator_worker: true,
    });
  const rows: (SeededJob & { sql: string })[] = [];
  const add = (
    state: SeededJob['state'],
    key: string,
    cols: { progress?: string; error?: string; attempt: number },
  ) => {
    const id = jobId();
    rows.push({
      id,
      state,
      key,
      text: cols.error ?? null,
      sql:
        `INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext, state, ` +
        `priority, attempt, progress, error, mount_id, created_at, updated_at, finished_at) VALUES ` +
        `(${id}, 0, 'mount_transcode', '${spec(key)}'::jsonb, '${key}', 734003200, 'mp4', '${state}', 5, ` +
        `${cols.attempt}, ${cols.progress ? `'${cols.progress}'::jsonb` : 'NULL'}, ` +
        `${cols.error ? `'${cols.error}'` : 'NULL'}, 1, now(), now(), ` +
        `${state === 'failed' ? 'now()' : 'NULL'})`,
    });
  };
  add('queued', `chat/1/1/${name}-queued.mp4`, { attempt: 0 });
  add('running', `chat/1/2/${name}-running.mp4`, {
    attempt: 1,
    progress: JSON.stringify({ stage: 'encode', percent: 42.5 }),
  });
  add('failed', `chat/1/3/${name}-failed.mp4`, {
    attempt: 3,
    error: `${name} could not read the source ${name}-an-unbroken-file-name-of-a-recording-from-the-weekend-movie-night-take-two.mp4 (the storage endpoint answered 403 Forbidden)`,
  });
  for (const r of rows) psql(db, r.sql);
  return {
    suffix: sfx,
    jobs: rows.map(({ sql: _sql, ...j }) => j),
    remove: () =>
      psql(db, `DELETE FROM transcode_jobs WHERE id IN (${rows.map((r) => r.id).join(',')})`),
  };
}

export type ArcadeTier = 'TIER_1_ORIGINAL' | 'TIER_2_REIMPLEMENTATION';
export interface SeededTier {
  id: string;
  title: string;
  summary: string;
  tier: ArcadeTier;
  repoUrl: string;
  /** Delete the entry again (the Operator's route). */
  remove: () => Promise<void>;
}

/**
 * An Arcade "Coming soon" game with a content tier. Default tier: the reimplementation (the one that also shows the
 * "Desktop only" and "Needs the base game" meta line on the card).
 */
export async function seedActivityTier(
  ctx: SeedCtx,
  tier: ArcadeTier = 'TIER_2_REIMPLEMENTATION',
): Promise<SeededTier> {
  if (!ctx.owner) throw new Error('seedActivityTier needs ctx.owner (the Owner session)');
  const sfx = suffix();
  const title = `${PREFIX}${sfx}-${tier === 'TIER_1_ORIGINAL' ? 'original' : 'reimpl'}`;
  const summary = `${title}: a game that is not out yet, listed by the p100 seed with its tier.`;
  const repoUrl = `https://example.com/${PREFIX}${sfx}`;
  const owner = ctx.owner;
  const card = await apiPost<{ id: string; title: string; tier: ArcadeTier }>(
    owner,
    '/admin/arcade/coming-soon',
    { title, summary, tier, repo_url: repoUrl },
  );
  return {
    id: card.id,
    title: card.title,
    summary,
    tier: card.tier,
    repoUrl,
    remove: async () => {
      await apiDelete(owner, `/admin/arcade/coming-soon/${card.id}`);
    },
  };
}

// ---- wave 108 (D-1026): what the sweep world gained for the second census ----------------------------------------

/** The ids a spec finds in `world.json` under the key `extras` (D-1026). Every name carries `sweep-x-`/`Sweep X` and a random suffix. */
export interface SweepExtras {
  /** A second account, an accepted friend of the Owner. */
  friendId: string | null;
  friendName: string | null;
  /** A third account whose friend request to the Owner is still pending (the Owner sees Accept). */
  askerId: string | null;
  askerName: string | null;
  /** The group DM of the Owner and the two new accounts, with a name and one message from each. */
  groupDmId: string | null;
  groupDmName: string | null;
  /** A text channel beside the seeded one (so no message is added to the seeded channel). */
  extrasChannelId: string | null;
  extrasChannelName: string | null;
  /** In that channel: a message with 14 reactions (wraps to two lines at 375), and one with a ||spoiler||. */
  reactionsMessageId: string | null;
  spoilerMessageId: string | null;
  /** A comment with the spoiler tag on the world's Cinema title. */
  spoilerCommentId: string | null;
  spoilerCommentTitleId: string | null;
  /** A private Library list with nothing in it, so its page shows the empty-state links. */
  listId: string | null;
  listName: string | null;
  /** What this seed does not make, with the reason (kept in the world so a census can say it). */
  notSeeded: string[];
}

export interface SweepExtrasIn {
  browser: import('@playwright/test').Browser;
  /** The Owner's and the member's request contexts (both signed in). */
  owner: APIRequestContext;
  member: APIRequestContext;
  guildId: string;
  titleId: string;
  origin: string;
}

const REACTION_EMOJI = [
  '👍',
  '❤️',
  '😂',
  '🎉',
  '🔥',
  '👀',
  '🙏',
  '😮',
  '😢',
  '🚀',
  '💯',
  '✅',
  '🌟',
  '🍿',
];

/**
 * D-1026: the data the censuses of wave 106 could not see. Each step prints its own `ok` or `FAIL` line into `log`; a
 * step the API refuses is a FAIL with the status, never skipped. Nothing existing is changed: new accounts, a new text
 * channel, a new group DM, one more comment and one new list. The accounts are two (the seed already registers one: three
 * a minute against the limit of five; `registerUser` waits out a 429).
 */
export async function seedSweepExtras(i: SweepExtrasIn, log: string[]): Promise<SweepExtras> {
  const t0 = Date.now();
  const sfx = suffix();
  const x: SweepExtras = {
    friendId: null,
    friendName: null,
    askerId: null,
    askerName: null,
    groupDmId: null,
    groupDmName: null,
    extrasChannelId: null,
    extrasChannelName: null,
    reactionsMessageId: null,
    spoilerMessageId: null,
    spoilerCommentId: null,
    spoilerCommentTitleId: null,
    listId: null,
    listName: null,
    notSeeded: [],
  };
  const step = async <T>(name: string, fn: () => Promise<T>): Promise<T | null> => {
    try {
      const v = await fn();
      log.push(`ok   ${name}`);
      return v;
    } catch (e) {
      log.push(`FAIL ${name}: ${String(e).slice(0, 240)}`);
      return null;
    }
  };
  const sleep = (ms: number) => new Promise((ok) => setTimeout(ok, ms));
  const o = i.owner;
  const ownerId = await step('x: owner id', async () => {
    const r = await o.get('/api/v1/users/@me');
    return ((await r.json()) as { id: string }).id;
  });
  const react = (r: APIRequestContext, channel: string, message: string, emoji: string) =>
    apiPut(
      r,
      `/channels/${channel}/messages/${message}/reactions/${encodeURIComponent(emoji)}/@me`,
    );

  // (1) two accounts: one accepted friend, one pending request to the Owner.
  const account = async (label: string, display: string) => {
    const anon = await i.browser.newContext({ baseURL: i.origin });
    try {
      const u = await registerUser(anon.request, label);
      const ctx = await i.browser.newContext({ baseURL: i.origin });
      await loginApi(ctx.request, u);
      await apiPatch(ctx.request, '/users/@me', { global_name: display });
      return { id: u.id, name: display, ctx };
    } finally {
      await anon.close();
    }
  };
  const friend = await step('x: friend account', () => account('sweepfriend', 'Marlow'));
  const asker = await step('x: requester account', () => account('sweepasker', 'Tess'));
  try {
    if (friend && ownerId) {
      x.friendId = friend.id;
      x.friendName = friend.name;
      await step('x: accepted friend', async () => {
        await apiPut(o, `/users/@me/relationships/${friend.id}`, { type: 1 });
        await apiPut(friend.ctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
      });
    }
    if (asker && ownerId) {
      x.askerId = asker.id;
      x.askerName = asker.name;
      await step('x: pending friend request to the Owner', () =>
        apiPut(asker.ctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 }),
      );
    }

    // (2) a group DM of the three.
    if (friend && asker) {
      x.groupDmName = `Sweep X crew ${sfx}`;
      x.groupDmId = await step('x: group dm', async () => {
        const ch = await apiPost<{ id: string }>(o, '/users/@me/channels', {
          recipients: [friend.id, asker.id],
        });
        await apiPatch(o, `/channels/${ch.id}`, { name: x.groupDmName });
        await sleep(1200);
        await apiPost(o, `/channels/${ch.id}/messages`, { content: 'Welcome to the crew chat.' });
        await sleep(1200);
        await apiPost(friend.ctx.request, `/channels/${ch.id}/messages`, {
          content: 'Friday still on? I can bring snacks.',
        });
        await sleep(1200);
        await apiPost(asker.ctx.request, `/channels/${ch.id}/messages`, {
          content: 'Count me in.',
        });
        return ch.id;
      });
    }
  } finally {
    await friend?.ctx.close();
    await asker?.ctx.close();
  }

  // (3) a text channel beside the seeded one: a message with 14 reactions and one with a spoiler.
  x.extrasChannelName = `sweep-x-${sfx}`;
  const ch = await step('x: extras channel', () =>
    apiPost<{ id: string }>(o, `/guilds/${i.guildId}/channels`, {
      name: x.extrasChannelName,
      type: 0,
    }),
  );
  if (ch) {
    x.extrasChannelId = ch.id;
    await sleep(1200);
    x.reactionsMessageId = await step('x: message with 14 reactions', async () => {
      const m = await apiPost<{ id: string }>(o, `/channels/${ch.id}/messages`, {
        content: 'Which snack should we get? Vote with a reaction.',
      });
      for (const e of REACTION_EMOJI) {
        await react(o, ch.id, m.id, e);
        await sleep(250);
      }
      // The member adds to three of them, so the pills carry different counts.
      for (const e of REACTION_EMOJI.slice(0, 3)) {
        await react(i.member, ch.id, m.id, e);
        await sleep(250);
      }
      return m.id;
    });
    await sleep(1200);
    const sm = await step('x: message with a spoiler', () =>
      apiPost<{ id: string }>(o, `/channels/${ch.id}/messages`, {
        content: 'The ending of the film: ||it was the harbour all along||.',
      }),
    );
    x.spoilerMessageId = sm?.id ?? null;
  }
  x.notSeeded.push(
    "attachments and images on a message: an upload needs an object in storage and this stack's storage is `memory:` (presigned PUTs are fake `memory://` URLs; message create heads the object and refuses a missing one)",
  );

  // (4) a Cinema comment with a spoiler, from the member, on the world's title.
  const c = await step('x: cinema comment with a spoiler', () =>
    apiPost<{ id: string }>(i.member, `/cinema/titles/${i.titleId}/comments`, {
      body: `Sweep X ${sfx}: the twist at the harbour is the best part.`,
      spoiler: true,
    }),
  );
  if (c) {
    x.spoilerCommentId = c.id;
    x.spoilerCommentTitleId = i.titleId;
  }

  // (5) a Library list (the world had none). A Study file needs an uploaded object: not seedable.
  x.listName = `Sweep X night picks ${sfx}`;
  const l = await step('x: library list', () =>
    apiPost<{ id: string }>(o, '/library/lists', { name: x.listName, visibility: 'private' }),
  );
  x.listId = l?.id ?? null;
  x.notSeeded.push(
    'a Study file: files are objects on a storage shelf (a bucket); none can be written on `memory:` storage',
    'a call in progress: needs a media server (LiveKit) setting',
    'a playable video (player control bar): needs a media file in storage',
    'a Live TV channel on the air: needs scheduled programs with playable media',
    'a bucket for the tier image picker: needs a storage bucket setting',
    'the epub and pdf readers: need a Library upload (a file in storage)',
  );
  log.push(`ok   x: extras took ${Math.round((Date.now() - t0) / 1000)} s`);
  return x;
}
