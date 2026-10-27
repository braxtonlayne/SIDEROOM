// Data layer for the transcoding admin screens (CIN-ADM-002, ADM-CNT-006): worker tokens, workers with drain,
// the job queue, job logs, retry and re-transcode.
//
// The staff routes live in crates/sr-api/src/routes/cinema_media/transcode.rs and staff.rs. The screens use a
// friendlier shape than the wire (`id` for `job_id`, `percent` for `progress.percent`), so every call below maps
// the wire shape to it. A server without these routes still gets the "not available yet" state on every tab.
//
//   GET    /admin/transcode/workers                      -> WorkerView[]
//   PUT    /admin/transcode/workers/{id}/drain           {drain} -> 204
//   GET    /admin/transcode/tokens                       -> WorkerToken[]
//   POST   /admin/transcode/tokens                       {name} -> {token, secret}   (secret shown once)
//   DELETE /admin/transcode/tokens/{id}                  -> 204 (revokes)
//   GET    /admin/transcode/queue                        -> {queued, running, failed_last_day, succeeded_last_day}
//   GET    /admin/transcode/jobs?state=&before=&limit=   -> JobSummary[]  (newest first; `before` is a job id)
//   GET    /admin/transcode/jobs/{id}                    -> JobDetail (log tail)
//   POST   /admin/transcode/jobs/{id}/cancel             -> 204
//   POST   /admin/transcode/jobs/{id}/retry              -> 204
//   GET    /admin/cinema/assets/{asset_id}               -> {asset, jobs, probe}
//   POST   /admin/transcode/retranscode                  {scope: asset|title|all, id?, reason?} -> outcome
//
// Permission: MANAGE_TRANSCODE (doc 07). Ids are Snowflake strings, timestamps Unix milliseconds.
import { ApiError, apiFetch } from '../../api/client';

const enc = encodeURIComponent;
const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });

/** The route itself is missing (an app-level 404 carries a `code`; the router's fallback does not). */
export function isUnavailable(e: unknown): boolean {
  return (
    e instanceof ApiError &&
    (e.status === 405 || e.status === 501 || (e.status === 404 && !e.body?.code))
  );
}

export interface WorkerCapabilities {
  nvenc?: boolean;
  libplacebo?: boolean;
  encoders?: string[];
  concurrency?: number;
}
export interface Worker {
  id: string;
  name: string;
  version: string;
  capabilities: WorkerCapabilities;
  last_seen_ms: number;
  online: boolean;
  drain: boolean;
  active_jobs: number;
  token_name?: string;
  token_revoked?: boolean;
}
export interface WorkerToken {
  id: string;
  name: string;
  created_at_ms: number;
  revoked_at_ms?: number | null;
  last_used_ms?: number | null;
}
export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';
export const JOB_STATES: JobState[] = ['queued', 'running', 'succeeded', 'failed', 'cancelled'];
export interface TranscodeJob {
  id: string;
  asset_id: string;
  /** The storage mount a mount_transcode job works in (BYOS-CNV-003); absent for the Cinema kinds. */
  mount_id?: string | null;
  kind: 'proxy' | 'ladder' | 'remux' | string;
  state: JobState;
  priority: number;
  attempt: number;
  worker_id?: string | null;
  worker_name?: string | null;
  stage?: string | null;
  percent?: number | null;
  error?: string | null;
  cancel_requested?: boolean;
  created_at_ms: number;
  finished_at_ms?: number | null;
}
export interface TranscodeJobDetail extends TranscodeJob {
  log_tail?: string | null;
  encoder?: string | null;
}
export interface AssetTranscode {
  asset_id: string;
  state: string;
  duration_ms?: number | null;
  profile_version?: number | null;
  jobs: TranscodeJob[];
}
export interface QueueCounts {
  queued: number;
  running: number;
  failed_last_day: number;
  succeeded_last_day: number;
}

interface WireJob {
  job_id: string;
  asset_id: string;
  mount_id?: string | null;
  kind: string;
  state: JobState;
  priority: number;
  attempt: number;
  worker_id?: string | null;
  cancel_requested: boolean;
  progress?: { stage?: string | null; percent?: number | null } | null;
  error?: string | null;
  created_at_ms: number;
  finished_at_ms?: number | null;
  log_tail?: string | null;
  encoder?: string | null;
}
const toJob = (j: WireJob): TranscodeJob => ({
  id: j.job_id,
  asset_id: j.asset_id,
  mount_id: j.mount_id ?? null,
  kind: j.kind,
  state: j.state,
  priority: j.priority,
  attempt: j.attempt,
  worker_id: j.worker_id ?? null,
  stage: j.progress?.stage ?? null,
  percent: j.progress?.percent ?? null,
  error: j.error ?? null,
  cancel_requested: j.cancel_requested,
  created_at_ms: j.created_at_ms,
  finished_at_ms: j.finished_at_ms ?? null,
});

export const listWorkers = async () => ({
  items: await get<Worker[]>('/admin/transcode/workers'),
});
export const setDrain = (id: string, drain: boolean) =>
  send<void>(`/admin/transcode/workers/${enc(id)}/drain`, 'PUT', { drain });

type WireToken = Omit<WorkerToken, 'last_used_ms'> & { last_used_at_ms?: number | null };
export const listTokens = async () => {
  const rows = await get<WireToken[]>('/admin/transcode/tokens');
  return {
    items: rows.map(({ last_used_at_ms, ...r }): WorkerToken => ({
      ...r,
      last_used_ms: last_used_at_ms ?? null,
    })),
  };
};
export const createToken = (name: string) =>
  send<{ token: WorkerToken; secret: string }>('/admin/transcode/tokens', 'POST', { name });
export const revokeToken = (id: string) =>
  send<void>(`/admin/transcode/tokens/${enc(id)}`, 'DELETE');

export const getQueueCounts = () => get<QueueCounts>('/admin/transcode/queue');

export const listJobs = async (p: { state?: string; after?: string | null; limit?: number }) => {
  const limit = p.limit ?? 50;
  const q = new URLSearchParams({ limit: String(limit) });
  if (p.state) q.set('state', p.state);
  if (p.after) q.set('before', p.after);
  const rows = await get<WireJob[]>(`/admin/transcode/jobs?${q.toString()}`);
  const items = rows.map(toJob);
  return { items, next_after: rows.length >= limit ? items[items.length - 1]!.id : null };
};
export const getJob = async (id: string): Promise<TranscodeJobDetail> => {
  const j = await get<WireJob>(`/admin/transcode/jobs/${enc(id)}`);
  return { ...toJob(j), log_tail: j.log_tail ?? null, encoder: j.encoder ?? null };
};
export const cancelJob = (id: string) =>
  send<void>(`/admin/transcode/jobs/${enc(id)}/cancel`, 'POST');
export const retryJob = (id: string) =>
  send<void>(`/admin/transcode/jobs/${enc(id)}/retry`, 'POST');

export const getAssetTranscode = async (assetId: string): Promise<AssetTranscode> => {
  const r = await get<{
    asset: {
      id: string;
      state: string;
      duration_ms?: number | null;
      profile_version?: number | null;
    };
    jobs: WireJob[];
  }>(`/admin/cinema/assets/${enc(assetId)}`);
  return {
    asset_id: r.asset.id,
    state: r.asset.state,
    duration_ms: r.asset.duration_ms ?? null,
    profile_version: r.asset.profile_version ?? null,
    jobs: r.jobs.map(toJob),
  };
};
export const retranscodeAsset = (assetId: string, reason?: string) =>
  send<{ enqueued: string[]; skipped: unknown[]; failed: unknown[] }>(
    '/admin/transcode/retranscode',
    'POST',
    { scope: 'asset', id: assetId, reason: reason || null },
  );
