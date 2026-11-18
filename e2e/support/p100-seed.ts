import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import type { APIRequestContext } from '@playwright/test';
import { DATABASE_URL } from './cli';
import { apiPost } from './fixtures';
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
