// REST helpers for Admin > Library catalog: /api/v1/admin/library/* (PUBLISH_READING; merge also needs
// DELETE_CONTENT). Shapes mirror crates/sr-library/src/model.rs.
import { apiFetch } from '../../api/client';

export { errorText } from '../../settings/server/api';

const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });
const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

export const SERIES_KINDS = [
  'manga',
  'manhwa',
  'manhua',
  'webtoon',
  'comic',
  'novel',
  'book',
] as const;
export const SERIES_STATUSES = ['ongoing', 'completed', 'hiatus', 'cancelled'] as const;
export const DIRECTIONS = ['rtl', 'ltr', 'vertical'] as const;
export type SeriesKind = (typeof SERIES_KINDS)[number];
export type SeriesStatus = (typeof SERIES_STATUSES)[number];
export type Direction = (typeof DIRECTIONS)[number];

export interface Series {
  id: string;
  kind: SeriesKind;
  title: string;
  alt_titles: string[];
  authors: string[];
  artists: string[];
  status: SeriesStatus;
  year?: number | null;
  synopsis: string;
  genres: string[];
  tags: string[];
  reading_direction: Direction;
  age_restricted: boolean;
  cover_url?: string | null;
  published: boolean;
  published_at_ms?: number | null;
  merged_into?: string | null;
  chapter_count: number;
  last_chapter_at_ms?: number | null;
  follower_count: number;
  created_at_ms: number;
  updated_at_ms: number;
}
export interface SeriesPage {
  items: Series[];
  total: number;
  limit: number;
  offset: number;
}
export interface SeriesInput {
  kind: SeriesKind;
  title: string;
  alt_titles: string[];
  authors: string[];
  artists: string[];
  status: SeriesStatus;
  year: number | null;
  synopsis: string;
  genres: string[];
  tags: string[];
  reading_direction: Direction;
  age_restricted: boolean;
  cover_key?: string | null;
}
export interface Edition {
  id: string;
  series_id: string;
  language: string;
  label: string;
  is_default: boolean;
}
export interface Volume {
  id: string;
  edition_id: string;
  number: number;
  title?: string | null;
}
export interface Chapter {
  id: string;
  series_id: string;
  edition_id: string;
  volume_id?: string | null;
  number: number;
  title?: string | null;
  page_count: number;
  format: 'images' | 'pdf' | 'epub';
  release_group?: string | null;
  published: boolean;
  published_at_ms?: number | null;
}
export interface ChapterList {
  editions: Edition[];
  volumes: Volume[];
  chapters: Chapter[];
  next_cursor?: string | null;
}

export const listSeries = (p: {
  title?: string;
  published?: boolean;
  kind?: string;
  status?: string;
  limit: number;
  offset: number;
}) => get<SeriesPage>(`/admin/library/series${qs({ ...p, sort: 'updated' })}`);
export const getSeries = (id: string) => get<Series>(`/admin/library/series/${id}`);
export const createSeries = (b: SeriesInput) => send<Series>('/admin/library/series', 'POST', b);
export const updateSeries = (id: string, b: SeriesInput) =>
  send<Series>(`/admin/library/series/${id}`, 'PUT', b);
export const deleteSeries = (id: string) => send<void>(`/admin/library/series/${id}`, 'DELETE');
export const setSeriesPublished = (id: string, published: boolean) =>
  send<Series>(`/admin/library/series/${id}/${published ? 'publish' : 'unpublish'}`, 'POST');
export const mergeSeries = (id: string, target_id: string) =>
  send<Series>(`/admin/library/series/${id}/merge`, 'POST', { target_id });
export const listChapters = (seriesId: string) =>
  get<ChapterList>(`/admin/library/series/${seriesId}/chapters`);
export const createEdition = (
  seriesId: string,
  b: { language: string; label: string; is_default: boolean },
) => send<Edition>(`/admin/library/series/${seriesId}/editions`, 'POST', b);
export const deleteEdition = (id: string) => send<void>(`/admin/library/editions/${id}`, 'DELETE');
export const createChapter = (
  editionId: string,
  b: {
    number: number;
    title: string | null;
    format: 'images' | 'pdf' | 'epub';
    release_group: string | null;
  },
) => send<Chapter>(`/admin/library/editions/${editionId}/chapters`, 'POST', b);
export const deleteChapter = (id: string) => send<void>(`/admin/library/chapters/${id}`, 'DELETE');
export const setChapterPublished = (id: string, published: boolean) =>
  send<Chapter>(`/admin/library/chapters/${id}/${published ? 'publish' : 'unpublish'}`, 'POST');

/* ------------------------------------------------ pages, uploads, credits (P13) */

export interface AdminPage {
  index: number;
  width: number;
  height: number;
  is_double_spread: boolean;
  blurhash?: string | null;
  crop?: { left: number; top: number; right: number; bottom: number } | null;
  /** Signed URL of the full-size page; its path after /cdn/ is the storage key. */
  url: string;
  variants: { width: number; url: string }[];
}
export interface AdminManifest {
  chapter: Chapter;
  pages: AdminPage[];
}
export interface PageInput {
  key: string;
  variants: Record<string, string>;
  width: number;
  height: number;
  is_double_spread: boolean;
  blurhash?: string | null;
  crop?: AdminPage['crop'];
}

/** The storage key inside a signed /cdn/{key}?... URL. */
export const keyOfUrl = (url: string) => {
  const path = url.split('?')[0] ?? '';
  const at = path.indexOf('/cdn/');
  return at >= 0 ? decodeURIComponent(path.slice(at + 5)) : path;
};

/** A page as the PUT pages body wants it. */
export const toPageInput = (p: AdminPage): PageInput => ({
  key: keyOfUrl(p.url),
  variants: Object.fromEntries(p.variants.map((v) => [String(v.width), keyOfUrl(v.url)])),
  width: p.width,
  height: p.height,
  is_double_spread: p.is_double_spread,
  blurhash: p.blurhash ?? null,
  crop: p.crop ?? null,
});

export const getPages = (chapterId: string) =>
  get<AdminManifest>(`/admin/library/chapters/${chapterId}/pages`);
export const replacePages = (chapterId: string, pages: PageInput[]) =>
  send<Chapter>(`/admin/library/chapters/${chapterId}/pages`, 'PUT', pages);

/** `PUT /admin/library/chapters/{id}/pages/{index}`: swap one page for a new image (a data URI). */
export const replacePage = (chapterId: string, index: number, image: string) =>
  send<Chapter>(`/admin/library/chapters/${chapterId}/pages/${index}`, 'PUT', { image });

export type UploadStatus = 'awaiting_upload' | 'queued' | 'running' | 'done' | 'failed' | 'expired';
export interface Upload {
  id: string;
  chapter_id: string;
  filename: string;
  status: UploadStatus;
  error?: string | null;
  warnings: string[];
  page_count?: number | null;
}
export interface UploadSlot {
  upload: Upload;
  upload_url: string;
  upload_method: string;
  max_bytes: number;
}
export const beginUpload = (chapterId: string, file: { name: string; size: number }) =>
  send<UploadSlot>(`/admin/library/chapters/${chapterId}/uploads`, 'POST', {
    filename: file.name,
    size_bytes: file.size,
  });
export const completeUpload = (uploadId: string) =>
  send<Upload>(`/admin/library/uploads/${uploadId}/complete`, 'POST');
export const uploadStatus = (uploadId: string) => get<Upload>(`/admin/library/uploads/${uploadId}`);

export interface Credit {
  id: string;
  section: string;
  ref_id: string;
  user_id?: string | null;
  submission_id?: string | null;
  label: string;
  granted_at_ms: number;
  revoked_at_ms?: number | null;
  revoke_reason?: string | null;
}
export const listCredits = (submissionId: string) =>
  get<Credit[]>(`/admin/review/submissions/${submissionId}/credits`);
export const revokeCredits = (submissionId: string, reason: string) =>
  send<{ revoked: number }>(`/admin/review/submissions/${submissionId}/revoke-credits`, 'POST', {
    reason,
  });

export interface CommentReport {
  id: string;
  reporter_id: string;
  reason: string;
  status: string;
  created_at_ms: number;
  comment?: { id: string; body: string; author_name: string; series_id: string } | null;
}
export const listCommentReports = () =>
  get<{ items: CommentReport[] }>('/admin/library/comment-reports');
export const resolveCommentReport = (id: string, hide: boolean) =>
  send<void>(`/admin/library/comment-reports/${id}/resolve`, 'POST', { hide });

// ---- featured reading lists (ADM-CNT-004) ----
export interface FeaturedList {
  id: string;
  name: string;
  owner_name: string;
  item_count: number;
  featured: boolean;
}
/** Public lists staff featured on the Library home, newest feature first. */
export const featuredLists = () =>
  get<{ items: FeaturedList[] }>(`/library/lists/featured?limit=50`).then((r) => r.items);
export const setListFeatured = (id: string, featured: boolean) =>
  send<void>(`/admin/library/lists/${encodeURIComponent(id)}/feature`, 'POST', { featured });
