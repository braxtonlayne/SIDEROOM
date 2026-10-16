// Phase 13 review routes: duplicates, review media, metadata lookup, resumable uploads,
// archive inspection and ingest sources. Mutations never retry so the person sees the error.
import { apiFetch } from '../api/client';
import { qs } from './api';
import type { Upload, UploadSlot } from './types';

const enc = encodeURIComponent;
const post = <T>(path: string, json?: unknown) =>
  apiFetch<T>(path, { method: 'POST', retries: 0, ...(json !== undefined ? { json } : {}) });
const put = <T>(path: string, json: unknown) =>
  apiFetch<T>(path, { method: 'PUT', retries: 0, json });

// ---- duplicates ----

export interface DuplicateMatch {
  section: string;
  ref_id: string;
  title: string;
  year: number | null;
  score: number;
  reasons: string[];
  href: string | null;
}
export interface DuplicateResolution {
  action: 'merge' | 'replacement' | 'not_duplicate';
  target: string | null;
  note: string | null;
  decided_by: string | null;
  decided_at_ms: number;
}
export interface DuplicateReport {
  matches: DuplicateMatch[];
  resolution: DuplicateResolution | null;
}
export const duplicatesMine = (id: string) =>
  apiFetch<DuplicateReport>(`/submissions/${enc(id)}/duplicates`);
export const duplicatesForReview = (id: string) =>
  apiFetch<DuplicateReport>(`/admin/review/submissions/${enc(id)}/duplicates`);
export const resolveDuplicate = (
  id: string,
  b: { action: DuplicateResolution['action']; target?: string; note?: string },
) => put<DuplicateReport>(`/admin/review/submissions/${enc(id)}/duplicate`, b);

// ---- video proxies ----

export interface ReviewMediaItem {
  ref_id: string;
  label: string;
  state: 'none' | 'processing' | 'ready' | 'failed';
  url: string | null;
  duration_ms: number | null;
}
export const reviewMedia = (id: string) =>
  apiFetch<{ items: ReviewMediaItem[] }>(`/admin/review/submissions/${enc(id)}/media`);

// ---- metadata lookup ----

export interface LookupCandidate {
  provider: string;
  external_id: string;
  title: string;
  alt_titles: string[];
  year: number | null;
  synopsis: string;
  authors: string[];
  genres: string[];
  url: string | null;
  cover: string;
}
export const lookupProviders = (section: 'library' | 'cinema') =>
  apiFetch<{ enabled: boolean; providers: { id: string; label: string }[] }>(
    `/submissions/lookup/providers${qs({ section })}`,
  );
export const lookup = (p: { provider: string; q: string; year?: number; section: string }) =>
  apiFetch<{ candidates: LookupCandidate[] }>(`/submissions/lookup${qs({ ...p })}`);

// ---- resumable uploads ----

export interface PartUrl {
  part_number: number;
  url: string;
}
export interface MultipartSlot {
  part_size: number;
  part_count: number;
  parts: PartUrl[];
}
export interface UploadParts {
  part_size: number;
  part_count: number;
  received: { part_number: number; size: number }[];
  missing: PartUrl[];
  expires_in_secs: number;
}
export type MultipartUploadSlot = UploadSlot & { multipart: MultipartSlot | null };
export const beginMultipart = (
  id: string,
  chapter: string,
  b: { filename: string; size_bytes: number },
) =>
  post<MultipartUploadSlot>(`/submissions/${enc(id)}/library/chapters/${enc(chapter)}/uploads`, {
    ...b,
    multipart: true,
  });
export const uploadParts = (id: string, upload: string) =>
  apiFetch<UploadParts>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}/parts`);
export const abortUpload = (id: string, upload: string) =>
  apiFetch<Upload>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}`, {
    method: 'DELETE',
    retries: 0,
  });
export const completeUpload = (id: string, upload: string) =>
  post<Upload>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}/complete`);

export interface ArchiveReport {
  format: string;
  size_bytes: number;
  pages: number;
  folders: { path: string; pages: number }[];
  first_pages: string[];
  skipped: string[];
  comic_info: { title: string | null; series: string | null; number: number | null } | null;
  has_series_json: boolean;
  problems: string[];
}
export const inspectUpload = (id: string, upload: string) =>
  apiFetch<ArchiveReport>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}/inspect`);

// ---- ingest sources ----

export interface IngestSource {
  id: string;
  label: string;
  kind: 'watched_folder' | 'storage_import';
  backend: 'local' | 'storage';
  location: string;
  owner_id: string;
  enabled: boolean;
  poll_secs: number;
  stable_secs: number;
  last_scan_at_ms: number | null;
  last_scan: Record<string, unknown> | null;
  last_error: string | null;
}
export interface PlannedChapter {
  rel_path: string;
  size_bytes: number;
  chapter: number | null;
  volume: number | null;
  title: string | null;
  status: string;
  note: string | null;
}
export interface PlannedSeries {
  title: string;
  existing_series: string | null;
  chapters: PlannedChapter[];
}
export interface ScanReport {
  dry_run: boolean;
  files_seen: number;
  series: PlannedSeries[];
  unplaced: PlannedChapter[];
  imported: number;
  waiting: number;
  failed: number;
  submissions: string[];
  problems: string[];
}
export const ingestSources = () =>
  apiFetch<{ sources: IngestSource[] }>('/admin/ingest-sources', { retries: 0 });
export const createIngestSource = (b: {
  label: string;
  kind: IngestSource['kind'];
  backend: IngestSource['backend'];
  location: string;
  owner_id: string;
}) => post<IngestSource>('/admin/ingest-sources', b);
export const patchIngestSource = (id: string, b: { enabled?: boolean }) =>
  apiFetch<IngestSource>(`/admin/ingest-sources/${enc(id)}`, {
    method: 'PATCH',
    retries: 0,
    json: b,
  });
export const deleteIngestSource = (id: string) =>
  apiFetch<void>(`/admin/ingest-sources/${enc(id)}`, { method: 'DELETE', retries: 0 });
export const dryRunIngestSource = (id: string) =>
  post<ScanReport>(`/admin/ingest-sources/${enc(id)}/dry-run`);
export const scanIngestSource = (id: string) =>
  post<ScanReport>(`/admin/ingest-sources/${enc(id)}/scan`);
