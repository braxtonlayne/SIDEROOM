// Data layer for the Live TV admin screens (TV-ADM-001..005): channels, schedule rules, generated schedule preview
// and publish, manual override ("now control"), and the what-aired report. Needs MANAGE_TV (doc 07).
//
// ASSUMED CONTRACT until p8-livetv-be documents "HTTP API as built" in docs/specs/cinema-live-tv.md (the spec
// names the viewer routes only). Change this file and the MSW fixtures when the real routes differ. A server
// without the routes (404/405/501 without an API error body) gets a "not available yet" state on every screen.
//
//   GET    /admin/tv/channels                                  -> {items: AdminChannel[]}
//   POST   /admin/tv/channels                                  ChannelInput -> AdminChannel
//   PUT    /admin/tv/channels/{id}                             ChannelInput -> AdminChannel
//   DELETE /admin/tv/channels/{id}                             -> 204
//   POST   /admin/tv/channels/{id}/logo                        {content_type} -> {key, upload_url, ...}
//   GET    /admin/tv/channels/{id}/schedule?from&to[&at]       -> SchedulePreview (generated, not yet published)
//   POST   /admin/tv/channels/{id}/schedule/publish            {from, to} -> SchedulePreview
//   GET    /admin/tv/channels/{id}/override                    -> {override, now}
//   POST   /admin/tv/channels/{id}/override                    {title_id, episode_id?, reason?} -> OverrideState
//   DELETE /admin/tv/channels/{id}/override                    -> 204 (back to the schedule)
//   POST   /admin/tv/channels/{id}/skip                        {reason?} -> OverrideState (next programme now)
//   GET    /admin/tv/channels/{id}/aired?from&to               -> {items: AiredEntry[]}
//
// Ids are Snowflake strings, timestamps Unix milliseconds.
import { ApiError, apiFetch } from '../../../api/client';
import type {
  ChannelCategory,
  ChannelKind,
  FillerPolicy,
  ProgramKind,
  TvBlock,
  TvChannel,
} from '../../../cinema/tv/types';

const enc = encodeURIComponent;
const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });
const qs = (p: Record<string, string | number | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

/** The route itself is missing (an app-level 404 carries a `code`; the router's fallback does not). */
export function isUnavailable(e: unknown): boolean {
  return (
    e instanceof ApiError &&
    (e.status === 405 || e.status === 501 || (e.status === 404 && !e.body?.code))
  );
}

export type { ChannelCategory, ChannelKind, FillerPolicy, ProgramKind };

export type Ordering = 'sequential' | 'shuffle' | 'weighted';
export const ORDERINGS: { value: Ordering; label: string }[] = [
  { value: 'sequential', label: 'Sequential per series' },
  { value: 'shuffle', label: 'Shuffle' },
  { value: 'weighted', label: 'Weighted' },
];
export const FILLER_LABELS: Record<FillerPolicy, string> = {
  bumpers: 'Bumpers (short clips tagged filler)',
  next_program_early: 'Start the next programme early',
  slate: 'Slate ("We’ll be right back")',
  off_air: 'Off air (color bars and tone)',
};

export interface Daypart {
  /** "HH:MM" in the channel's time zone. A start later than the end wraps past midnight (22:00 to 02:00). */
  start: string;
  end: string;
  /** Only age-restricted content airs in this window (needs an age-restricted channel). */
  restricted_only: boolean;
}

export interface ScheduleRules {
  pool: {
    genres: string[];
    kinds: Exclude<ProgramKind, 'filler'>[];
    decades: number[];
    tags: string[];
    collection: string | null;
  };
  block_minutes: number;
  ordering: Ordering;
  dayparts: Daypart[];
  rerun_spacing_hours: number;
}

export const EMPTY_RULES: ScheduleRules = {
  pool: { genres: [], kinds: [], decades: [], tags: [], collection: null },
  block_minutes: 30,
  ordering: 'sequential',
  dayparts: [],
  rerun_spacing_hours: 24,
};

export interface AdminChannel extends TvChannel {
  enabled: boolean;
  filler_policy: FillerPolicy;
  rules?: ScheduleRules | null;
  /** Schedule published up to here (null: nothing published yet). */
  published_until_ms?: number | null;
}

export interface ChannelInput {
  number: number;
  call_sign: string;
  name: string;
  network: string | null;
  era_label: string | null;
  logo_key?: string | null;
  kind: ChannelKind;
  category: ChannelCategory | null;
  timezone: string;
  age_restricted: boolean;
  enabled: boolean;
  filler_policy: FillerPolicy;
  rules: ScheduleRules | null;
}

export const listChannels = () => get<{ items: AdminChannel[] }>('/admin/tv/channels');
export const createChannel = (b: ChannelInput) =>
  send<AdminChannel>('/admin/tv/channels', 'POST', b);
export const updateChannel = (id: string, b: ChannelInput) =>
  send<AdminChannel>(`/admin/tv/channels/${enc(id)}`, 'PUT', b);
export const deleteChannel = (id: string) => send<void>(`/admin/tv/channels/${enc(id)}`, 'DELETE');

export interface LogoUpload {
  key: string;
  upload_url: string;
  content_type: string;
}
export const reserveLogo = (id: string, contentType: string) =>
  send<LogoUpload>(`/admin/tv/channels/${enc(id)}/logo`, 'POST', { content_type: contentType });

export interface PreviewBlock extends Pick<TvBlock, 'start_ms' | 'end_ms' | 'title' | 'kind'> {
  id?: string;
  episode_label?: string | null;
  /** Where the block comes from: the rules, a manual placement, an override or filler. */
  source?: 'rules' | 'manual' | 'guide' | 'override' | 'filler';
}
export interface SchedulePreview {
  channel_id: string;
  from_ms: number;
  to_ms: number;
  blocks: PreviewBlock[];
  generated_at_ms?: number;
  /** The generated schedule is what viewers see (true after a publish). */
  published: boolean;
  warnings?: string[];
}
export const previewSchedule = (id: string, from: number, to: number, at?: number) =>
  get<SchedulePreview>(`/admin/tv/channels/${enc(id)}/schedule${qs({ from, to, at })}`);
export const publishSchedule = (id: string, from: number, to: number) =>
  send<SchedulePreview>(`/admin/tv/channels/${enc(id)}/schedule/publish`, 'POST', { from, to });

export interface OverrideState {
  override: {
    title_id: string;
    title: string;
    episode_label?: string | null;
    started_ms: number;
  } | null;
  now: PreviewBlock | null;
}
export const getOverride = (id: string) =>
  get<OverrideState>(`/admin/tv/channels/${enc(id)}/override`);
export const setOverride = (
  id: string,
  b: { title_id: string; episode_id?: string | null; reason?: string },
) => send<OverrideState>(`/admin/tv/channels/${enc(id)}/override`, 'POST', b);
export const clearOverride = (id: string) =>
  send<void>(`/admin/tv/channels/${enc(id)}/override`, 'DELETE');
export const skipProgramme = (id: string, reason?: string) =>
  send<OverrideState>(`/admin/tv/channels/${enc(id)}/skip`, 'POST', reason ? { reason } : {});

export interface AiredEntry extends PreviewBlock {
  /** Peak concurrent viewers while it aired, when the server counts them. */
  peak_viewers?: number | null;
}
export const getAired = (id: string, from: number, to: number) =>
  get<{ items: AiredEntry[] }>(`/admin/tv/channels/${enc(id)}/aired${qs({ from, to })}`);

/** Staff title search for the override picker (the catalog's own admin route). */
export { listTitles } from '../api';
