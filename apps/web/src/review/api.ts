// Typed fetchers for the submissions routes (/submissions, /requests) and the reviewer routes
// (/admin/review/*). Mutations never retry on 429 so the person sees the error.
import { apiFetch } from '../api/client';
import type {
  ContentRequest,
  NamingItem,
  CreateSubmission,
  DraftChapter,
  DraftChapterList,
  KindInfo,
  LibraryPreview,
  RequestPage,
  ReviewPageManifest,
  SubmissionDetail,
  SubmissionListParams,
  SubmissionPage,
  Upload,
  UploadSlot,
} from './types';

type Query = Record<string, string | number | boolean | undefined | null>;

export function qs(q: Query): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) {
    if (v === undefined || v === null || v === '') continue;
    p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : '';
}

const enc = encodeURIComponent;
const post = <T>(path: string, json?: unknown) =>
  apiFetch<T>(path, { method: 'POST', retries: 0, ...(json !== undefined ? { json } : {}) });
const put = <T>(path: string, json: unknown) =>
  apiFetch<T>(path, { method: 'PUT', retries: 0, json });

// ---- submitter ----

export const listKinds = () => apiFetch<KindInfo[]>('/submissions/kinds');
export const listMine = (p: SubmissionListParams = {}) =>
  apiFetch<SubmissionPage>(`/submissions${qs({ ...p })}`);
export const createSubmission = (b: CreateSubmission) => post<SubmissionDetail>('/submissions', b);
export const getMine = (id: string) => apiFetch<SubmissionDetail>(`/submissions/${enc(id)}`);
export const updateMine = (id: string, b: { title?: string; summary?: string }) =>
  apiFetch<SubmissionDetail>(`/submissions/${enc(id)}`, { method: 'PATCH', retries: 0, json: b });
export const getMinePreview = (id: string) =>
  apiFetch<{ preview: LibraryPreview }>(`/submissions/${enc(id)}/preview`);
export const submit = (id: string) => post<SubmissionDetail>(`/submissions/${enc(id)}/submit`);
export const withdraw = (id: string) => post<SubmissionDetail>(`/submissions/${enc(id)}/withdraw`);
export const commentMine = (id: string, body: string) =>
  post<SubmissionDetail>(`/submissions/${enc(id)}/comments`, { body });

// Library draft content.
export const draftChapters = (id: string) =>
  apiFetch<DraftChapterList>(`/submissions/${enc(id)}/library/chapters`);
export const createDraftChapter = (
  id: string,
  b: { number: number; title?: string | null; format: string },
) => post<DraftChapter>(`/submissions/${enc(id)}/library/chapters`, b);
export const updateDraftChapter = (
  id: string,
  chapter: string,
  b: {
    number: number;
    title: string | null;
    format: string;
    release_group?: string | null;
    volume_id?: string | null;
  },
) =>
  apiFetch<DraftChapter>(`/submissions/${enc(id)}/library/chapters/${enc(chapter)}`, {
    method: 'PUT',
    json: b,
    retries: 0,
  });
export const deleteDraftChapter = (id: string, chapter: string) =>
  apiFetch<void>(`/submissions/${enc(id)}/library/chapters/${enc(chapter)}`, {
    method: 'DELETE',
    retries: 0,
  });
export const beginUpload = (
  id: string,
  chapter: string,
  b: { filename: string; size_bytes: number },
) => post<UploadSlot>(`/submissions/${enc(id)}/library/chapters/${enc(chapter)}/uploads`, b);
export const completeUpload = (id: string, upload: string) =>
  post<Upload>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}/complete`);
export const uploadStatus = (id: string, upload: string) =>
  apiFetch<Upload>(`/submissions/${enc(id)}/library/uploads/${enc(upload)}`);
export const chapterUploads = (id: string, chapter: string) =>
  apiFetch<{ uploads: Upload[] }>(
    `/submissions/${enc(id)}/library/chapters/${enc(chapter)}/uploads`,
  );

/** A blob as a `data:` URI (the cover route takes images that way, like Discord's icon and avatar fields). */
function blobToDataUri(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error ?? new Error('read failed'));
    r.readAsDataURL(blob);
  });
}
/**
 * Set the cover of my draft series (G-78): one JPEG or PNG up to 2 MiB; the server sniffs and re-encodes it. The
 * web app sends the first page of an uploaded PDF, rendered in the browser.
 */
export const setCover = async (id: string, image: Blob) =>
  post<unknown>(`/library/submissions/${enc(id)}/cover`, { image: await blobToDataUri(image) });

/**
 * Upload artwork into a slot of my draft (REV-ED-003): `cover` (new series), `poster` or `backdrop` (new
 * Cinema title). `image` is a `data:` URI; the server sniffs and re-encodes it. A reviewer who holds the
 * claim may do the same on a submission in review (D-255).
 */
export const setArtwork = (id: string, kind: 'cover' | 'poster' | 'backdrop', image: string) =>
  post<{ kind: string; url: string | null }>(`/submissions/${enc(id)}/artwork/${kind}`, { image });

/** PUT the file bytes to the presigned URL of an upload slot. */
export async function putFile(slot: UploadSlot, file: Blob): Promise<void> {
  const res = await fetch(slot.upload_url, { method: slot.upload_method || 'PUT', body: file });
  if (!res.ok) throw new Error(`The upload was refused (${res.status}).`);
}

export const parseNames = (section: string, names: string[]) =>
  post<{ items: NamingItem[] }>('/submissions/naming/parse', { section, names });

// ---- requests ----

export const listRequests = (
  p: { section?: string; status?: string; sort?: string; limit?: number; offset?: number } = {},
) => apiFetch<RequestPage>(`/requests${qs({ ...p })}`);
export const createRequest = (b: {
  section: string;
  title: string;
  creator?: string;
  year?: number;
  link?: string;
  note?: string;
}) => post<ContentRequest>('/requests', b);
export const voteRequest = (id: string, vote: boolean) =>
  vote
    ? put<ContentRequest>(`/requests/${enc(id)}/vote`, {})
    : apiFetch<ContentRequest>(`/requests/${enc(id)}/vote`, { method: 'DELETE', retries: 0 });

export const claimRequest = (id: string, note: string) =>
  put<ContentRequest>(`/requests/${enc(id)}/claim`, { note });
export const releaseRequest = (id: string) =>
  apiFetch<ContentRequest>(`/requests/${enc(id)}/claim`, { method: 'DELETE', retries: 0 });

// ---- reviewer ----

const R = '/admin/review';
export const reviewQueue = (p: SubmissionListParams = {}) =>
  apiFetch<SubmissionPage>(`${R}/queue${qs({ ...p })}`, { retries: 0 });
export const reviewGet = (id: string) =>
  apiFetch<SubmissionDetail>(`${R}/submissions/${enc(id)}`, { retries: 0 });
export const reviewPreview = (id: string) =>
  apiFetch<{ preview: LibraryPreview }>(`${R}/submissions/${enc(id)}/preview`, { retries: 0 });
export const reviewPages = (id: string, chapter: string) =>
  apiFetch<ReviewPageManifest>(
    `${R}/submissions/${enc(id)}/library/chapters/${enc(chapter)}/pages`,
    { retries: 0 },
  );
export const claim = (id: string, takeover = false) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/claim`, { takeover });
export const release = (id: string) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/release`);
export const reviewComment = (id: string, body: string, internal: boolean) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/comments`, { body, internal });
export const decideItem = (
  id: string,
  item: string,
  state: 'approved' | 'rejected' | 'pending',
  note?: string,
) => put<SubmissionDetail>(`${R}/submissions/${enc(id)}/items/${enc(item)}`, { state, note });
export const requestChanges = (id: string, reason: string) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/request-changes`, { reason });
export const reject = (id: string, reason: string) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/reject`, { reason });
export const approve = (id: string, reason?: string) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/approve`, { reason: reason ?? null });
export const publish = (id: string) =>
  post<SubmissionDetail>(`${R}/submissions/${enc(id)}/publish`);

/** Every message of a Discord-style field error tree (`errors.problems.0._errors[0].message`). */
export function errorMessages(e: unknown, fallback = 'Something went wrong. Try again.'): string[] {
  const out: string[] = [];
  const walk = (node: unknown) => {
    if (!node || typeof node !== 'object') return;
    for (const [k, v] of Object.entries(node as Record<string, unknown>)) {
      if (k === '_errors' && Array.isArray(v)) {
        for (const m of v)
          if (m && typeof (m as { message?: unknown }).message === 'string')
            out.push((m as { message: string }).message);
      } else walk(v);
    }
  };
  walk((e as { body?: { errors?: unknown } } | null)?.body?.errors);
  if (out.length) return out;
  const err = e as { status?: number; message?: string };
  if (err?.status === 403) return ['You do not have permission to do that.'];
  if (err?.status === 404) return ['That no longer exists.'];
  return [err?.message || fallback];
}

/** Edit the draft series of a new-series submission. */
export const updateSeries = (id: string, b: Record<string, unknown>) =>
  put<unknown>(`/submissions/${enc(id)}/library/series`, b);
