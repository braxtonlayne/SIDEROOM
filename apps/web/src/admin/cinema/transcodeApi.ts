// Data layer for the transcoding admin screens (CIN-ADM-002): worker tokens, workers with drain, the job queue,
// job logs and re-transcode.
//
// ASSUMED CONTRACT. The backend for this (p7-cinema-ingest-db) was not committed when these screens were written
// (docs/specs/cinema-media.md section 4.4 documents only the worker-facing API under /transcode/worker, not the
// staff one). The routes below are what the screens expect; when the real ones differ, change this file only.
// Every screen treats a missing route (404/405/501 without an API error body) as "not available yet" and says so
// instead of showing invented data.
//
//   GET    /admin/transcode/workers                      -> {items: Worker[]}
//   POST   /admin/transcode/workers/{id}/drain           {drain: boolean} -> Worker
//   GET    /admin/transcode/tokens                       -> {items: WorkerToken[]}
//   POST   /admin/transcode/tokens                       {name} -> {token: WorkerToken, secret}   (secret shown once)
//   DELETE /admin/transcode/tokens/{id}                  -> 204 (revokes)
//   GET    /admin/transcode/jobs?state=&after=&limit=    -> {items: TranscodeJob[], next_after?}
//   GET    /admin/transcode/jobs/{id}                    -> TranscodeJobDetail (log tail)
//   POST   /admin/transcode/jobs/{id}/cancel             -> TranscodeJob
//   GET    /admin/transcode/assets/{asset_id}            -> AssetTranscode
//   POST   /admin/transcode/assets/{asset_id}/retranscode {reason?} -> AssetTranscode
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
  kind: 'proxy' | 'ladder' | string;
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

export const listWorkers = () => get<{ items: Worker[] }>('/admin/transcode/workers');
export const setDrain = (id: string, drain: boolean) =>
  send<Worker>(`/admin/transcode/workers/${enc(id)}/drain`, 'POST', { drain });
export const listTokens = () => get<{ items: WorkerToken[] }>('/admin/transcode/tokens');
export const createToken = (name: string) =>
  send<{ token: WorkerToken; secret: string }>('/admin/transcode/tokens', 'POST', { name });
export const revokeToken = (id: string) =>
  send<void>(`/admin/transcode/tokens/${enc(id)}`, 'DELETE');
export const listJobs = (p: { state?: string; after?: string | null; limit?: number }) => {
  const q = new URLSearchParams();
  if (p.state) q.set('state', p.state);
  if (p.after) q.set('after', p.after);
  if (p.limit) q.set('limit', String(p.limit));
  const s = q.toString();
  return get<{ items: TranscodeJob[]; next_after?: string | null }>(
    `/admin/transcode/jobs${s ? `?${s}` : ''}`,
  );
};
export const getJob = (id: string) => get<TranscodeJobDetail>(`/admin/transcode/jobs/${enc(id)}`);
export const cancelJob = (id: string) =>
  send<TranscodeJob>(`/admin/transcode/jobs/${enc(id)}/cancel`, 'POST');
export const getAssetTranscode = (assetId: string) =>
  get<AssetTranscode>(`/admin/transcode/assets/${enc(assetId)}`);
export const retranscodeAsset = (assetId: string, reason?: string) =>
  send<AssetTranscode>(`/admin/transcode/assets/${enc(assetId)}/retranscode`, 'POST', {
    reason: reason || null,
  });
