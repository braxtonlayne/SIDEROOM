// REST helpers for the Community "more" tabs of Server Settings: guide, applications, insights, directory
// listing. Shapes mirror crates/sr-api/src/routes/{settings/guide,settings/applications,settings/insights,directory}.rs.
import { ApiError, apiFetch } from '../../api/client';
import { withReauth } from '../../admin/reauth';

const send = <T>(path: string, method: string, json?: unknown) =>
  withReauth(() =>
    apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) }),
  );

// ---- server guide (LNG-SSET-066) ----

export interface GuideTodo {
  id?: string | null;
  title: string;
  description?: string | null;
  channel_id?: string | null;
}
export interface GuideResource {
  id?: string | null;
  title: string;
  /** Markdown. */
  content: string;
}
export interface GuideObject {
  guild_id: string;
  enabled: boolean;
  welcome_message: string | null;
  todos: GuideTodo[];
  resources: GuideResource[];
}
export interface PutGuideBody {
  welcome_message: string | null;
  todos: GuideTodo[];
  resources: GuideResource[];
}
export const getGuide = (g: string) =>
  apiFetch<GuideObject>(`/guilds/${g}/sr/guide`, { retries: 0 });
export const putGuide = (g: string, body: PutGuideBody) =>
  send<GuideObject>(`/guilds/${g}/sr/guide`, 'PUT', body);

// ---- membership applications (LNG-SSET-042, LNG-SSET-039) ----

export interface ApplicationQuestion {
  id?: string | null;
  label: string;
  kind: 'short' | 'paragraph';
  required: boolean;
}
export interface ApplicationFormObject {
  guild_id: string;
  enabled: boolean;
  description: string | null;
  questions: ApplicationQuestion[];
}
export interface ApplicationAnswer {
  question_id: string;
  value: string;
}
export interface ApplicationObject {
  id: string;
  guild_id: string;
  user: { id: string; username: string; global_name?: string | null };
  answers: ApplicationAnswer[];
  status: 'pending' | 'approved' | 'rejected';
  created_at: string;
  reviewed_by: string | null;
  reviewed_at: string | null;
  rejection_reason: string | null;
}
export const getApplicationForm = (g: string) =>
  apiFetch<ApplicationFormObject>(`/guilds/${g}/sr/applications/form`, { retries: 0 });
export const putApplicationForm = (
  g: string,
  body: { enabled: boolean; description: string | null; questions: ApplicationQuestion[] },
) => send<ApplicationFormObject>(`/guilds/${g}/sr/applications/form`, 'PUT', body);
export const listApplications = (g: string, status?: ApplicationObject['status']) =>
  apiFetch<ApplicationObject[]>(
    `/guilds/${g}/sr/applications${status ? `?status=${status}` : ''}`,
    { retries: 0 },
  );
export const decideApplication = (
  g: string,
  applicationId: string,
  body: { approve: boolean; reason?: string },
) =>
  send<ApplicationObject>(`/guilds/${g}/sr/applications/${applicationId}/decision`, 'POST', body);

// ---- insights (LNG-SSET-068) ----

export interface InsightsDay {
  date: string;
  members: number;
  joins: number;
  messages: number;
  voice_minutes: number;
}
export interface InsightsObject {
  days: InsightsDay[];
  top_channels: { channel_id: string; messages: number }[];
}
export const getInsights = (g: string, days = 30) =>
  apiFetch<InsightsObject>(`/guilds/${g}/sr/insights?days=${days}`, { retries: 0 });

// ---- directory listing request (LNG-SSET-069) ----

export interface ListingObject {
  guild_id: string;
  /** `pending`, `approved`, `rejected` or `removed`. */
  status: string;
  category: string;
  description: string;
  featured: boolean;
  requested_at: string;
  decided_at: string | null;
  decision_note: string | null;
}
/** `null` when the server never asked (the API answers 404). */
export async function getOwnListing(g: string): Promise<ListingObject | null> {
  try {
    return await apiFetch<ListingObject>(`/guilds/${g}/directory`, { retries: 0 });
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) return null;
    throw e;
  }
}
export const requestListing = (g: string, body: { category: string; description: string }) =>
  send<ListingObject>(`/guilds/${g}/directory`, 'PUT', body);
export const withdrawListing = (g: string) => send<void>(`/guilds/${g}/directory`, 'DELETE');
export const listDirectoryCategories = () =>
  apiFetch<string[]>('/directory/categories', { retries: 0 });
