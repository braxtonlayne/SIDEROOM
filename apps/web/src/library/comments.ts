// Typed fetchers for comments, downloads, reading presence and the notifications bell
// (crates/sr-api/src/routes/library/{comments,downloads,presence}.rs). P13: LIB-SOC-003, LIB-RDR-014/016/017,
// LIB-SOC-007.
import { apiFetch } from '../api/client';
import { qs } from './api';

const enc = encodeURIComponent;

export interface LibComment {
  id: string;
  series_id: string;
  /** "0" for a comment on the series itself. */
  chapter_id: string;
  parent_id?: string | null;
  author_id: string;
  author_name: string;
  body: string;
  spoiler: boolean;
  like_count: number;
  reply_count: number;
  liked: boolean;
  mine: boolean;
  created_at_ms: number;
  edited_at_ms?: number | null;
}

export interface CommentPage {
  items: LibComment[];
  next_before?: string | null;
  /** Visible top-level comments and replies on the target. */
  total: number;
}

export type CommentTarget = { kind: 'series' | 'chapter'; id: string };

const base = (t: CommentTarget) =>
  t.kind === 'series'
    ? `/library/series/${enc(t.id)}/comments`
    : `/library/chapters/${enc(t.id)}/comments`;

export const getComments = (t: CommentTarget, o: { before?: string; limit?: number } = {}) =>
  apiFetch<CommentPage>(`${base(t)}${qs({ ...o })}`);

export const getReplies = (commentId: string, o: { before?: string; limit?: number } = {}) =>
  apiFetch<CommentPage>(`/library/comments/${enc(commentId)}/replies${qs({ ...o })}`);

export const postComment = (
  t: CommentTarget,
  body: { body: string; spoiler?: boolean; parent_id?: string },
) => apiFetch<LibComment>(base(t), { method: 'POST', json: body, retries: 0 });

export const editComment = (id: string, body: { body?: string; spoiler?: boolean }) =>
  apiFetch<LibComment>(`/library/comments/${enc(id)}`, { method: 'PATCH', json: body, retries: 0 });

export const deleteComment = (id: string) =>
  apiFetch<void>(`/library/comments/${enc(id)}`, { method: 'DELETE', retries: 0 });

export const likeComment = (id: string, on: boolean) =>
  apiFetch<{ count: number }>(`/library/comments/${enc(id)}/like`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });

export const reportComment = (id: string, reason: string) =>
  apiFetch<void>(`/library/comments/${enc(id)}/report`, {
    method: 'POST',
    json: { reason },
    retries: 0,
  });

export const commentKeys = {
  list: (t: CommentTarget) => ['library', 'comments', t.kind, t.id] as const,
  replies: (commentId: string) => ['library', 'comment-replies', commentId] as const,
};

/* ---------------------------------------------------------------- downloads */

export interface DownloadAllowance {
  entitled: boolean;
  per_day: number;
  left: number;
}

export interface ChapterExport {
  /** `cbz`, `pdf` or `epub`. */
  format: string;
  filename: string;
  url: string;
  size_bytes: number;
  expires_at: number;
}

export const getDownloadAllowance = () => apiFetch<DownloadAllowance>('/library/me/downloads');

export const downloadChapter = (chapterId: string) =>
  apiFetch<ChapterExport>(`/library/chapters/${enc(chapterId)}/download`, {
    method: 'POST',
    retries: 0,
  });

/* ----------------------------------------------------- presence and the bell */

export const readingHeartbeat = (chapterId: string) =>
  apiFetch<void>(`/library/chapters/${enc(chapterId)}/presence`, { method: 'PUT', retries: 0 });

export const readingStop = () =>
  apiFetch<void>('/library/me/presence', { method: 'DELETE', retries: 0 });

export const getNotificationCount = () => apiFetch<{ unread: number }>('/library/me/notifications');

export const markNotificationsSeen = () =>
  apiFetch<void>('/library/me/notifications/seen', { method: 'POST', retries: 0 });
