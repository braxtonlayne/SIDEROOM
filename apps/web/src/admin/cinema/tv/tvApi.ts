// Data layer for the Live TV admin screens (TV-ADM-001..005): channels, schedule rules, schedule preview and
// publish, "now" control, placing programmes, and the what-aired report. Needs MANAGE_TV (doc 07). The routes are
// docs/specs/cinema-live-tv.md section 8.5 (`/admin/tv/*`, crate sr-tv); ids are Snowflake strings, timestamps
// Unix milliseconds. Not on the server yet (the screens say so): guide import (TV-ADM-004), logo upload, the weekly
// grid editor (TV-ADM-003), imported_guide channels.
import { ApiError, apiFetch } from '../../../api/client';
import { toBlock, toChannel, type WireBlock, type WireChannel } from '../../../cinema/tv/api';
import type { FillerPolicy, TvBlock, TvChannel } from '../../../cinema/tv/types';

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

/** `GET /admin/tv/slate`: the off-air slate loop (L-14, D-269); it is made with `sr-cli tv slate`. */
export interface AdminSlate {
  configured: boolean;
  asset_id?: string;
  playable: boolean;
  duration_ms?: number;
}
export const getSlate = () => get<AdminSlate>('/admin/tv/slate');

/** The route itself is missing (an app-level 404 carries a `code`; the router's fallback does not). */
export function isUnavailable(e: unknown): boolean {
  return (
    e instanceof ApiError &&
    (e.status === 405 || e.status === 501 || (e.status === 404 && !e.body?.code))
  );
}

export type { FillerPolicy };
export const FILLER_LABELS: Record<FillerPolicy, string> = {
  bumpers: 'Bumpers (short clips tagged filler)',
  next_program_early: 'Start the next programme early',
  slate: 'Slate ("We’ll be right back")',
  off_air: 'Off air (color bars and tone)',
};

/* ------------------------------------------------------------------ rules */

export type PoolKind = 'movie' | 'series' | 'anime';
export type RestrictedMode = 'exclude' | 'include' | 'only';

export interface DaypartPool {
  kinds: PoolKind[];
  genres: string[];
  tags: string[];
  title_ids: string[];
  year_from: number | null;
  year_to: number | null;
  collection_id: string | null;
  min_runtime_min: number | null;
  max_runtime_min: number | null;
  restricted: RestrictedMode;
}
export interface Daypart {
  /** Unique within the channel; keys the sequential cursor. */
  name: string;
  /** "HH:MM" in the channel's time zone. An end at or before the start wraps past midnight. */
  start: string;
  end: string;
  /** 0 = Monday. Empty means every day. */
  days: number[];
  /** Sequential keeps counting across days (a series marathon). */
  mode: 'shuffle' | 'sequential';
  /** Overrun: the last programme may run past the end. Fit: only items that fit. */
  fit: 'overrun' | 'fit';
  pool: DaypartPool;
}
export interface ScheduleRules {
  /** Programmes start on this grid in channel time; 0 means back to back. */
  grid_minutes: number;
  /** Shuffle: the last N picks are not repeated (0 to 64). */
  no_repeat_window: number;
  dayparts: Daypart[];
}

export const EMPTY_POOL: DaypartPool = {
  kinds: [],
  genres: [],
  tags: [],
  title_ids: [],
  year_from: null,
  year_to: null,
  collection_id: null,
  min_runtime_min: null,
  max_runtime_min: null,
  restricted: 'exclude',
};
export const newDaypart = (name: string): Daypart => ({
  name,
  start: '00:00',
  end: '00:00',
  days: [],
  mode: 'shuffle',
  fit: 'overrun',
  pool: { ...EMPTY_POOL },
});
export const EMPTY_RULES: ScheduleRules = {
  grid_minutes: 30,
  no_repeat_window: 8,
  dayparts: [newDaypart('All day')],
};

/* --------------------------------------------------------------- channels */

export type AdminKind = 'rule_based' | 'manual' | 'imported_guide';
export interface AdminChannel extends TvChannel {
  enabled: boolean;
  filler_policy: FillerPolicy;
  rules: ScheduleRules | null;
  theme: string | null;
  seed: string | number | null;
  /** Key behind `logo_url` (`tv/...`), recovered from the signed URL so a save keeps the logo. */
  logo_key: string | null;
}
interface WireAdminChannel extends WireChannel {
  enabled: boolean;
  filler_policy: FillerPolicy;
  rules?: ScheduleRules | null;
  theme?: string | null;
  seed?: string | number | null;
}

/** The storage key behind a signed `/cdn/<key>?ex=&sig=` URL, when it is a TV key. */
export function logoKeyFromUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const path = new URL(url, 'http://local.invalid').pathname;
    const at = path.indexOf('/cdn/');
    if (at < 0) return null;
    const key = decodeURIComponent(path.slice(at + 5));
    return key.startsWith('tv/') ? key : null;
  } catch {
    return null;
  }
}

const toAdmin = (w: WireAdminChannel): AdminChannel => ({
  ...toChannel(w),
  enabled: w.enabled,
  filler_policy: w.filler_policy,
  rules: w.rules ?? null,
  theme: w.theme ?? null,
  seed: w.seed ?? null,
  logo_key: logoKeyFromUrl(w.logo_url),
});

export interface ChannelInput {
  number: number;
  name: string;
  call_sign: string | null;
  era_label: string | null;
  theme: string | null;
  logo_key?: string | null;
  enabled: boolean;
  kind: AdminKind;
  timezone: string;
  age_restricted: boolean;
  filler_policy: FillerPolicy;
  rules: ScheduleRules | null;
}

export const listChannels = async (): Promise<{ items: AdminChannel[] }> => {
  const r = await get<{ items: WireAdminChannel[] }>('/admin/tv/channels');
  return { items: r.items.map(toAdmin) };
};
export const createChannel = async (b: ChannelInput) =>
  toAdmin(await send<WireAdminChannel>('/admin/tv/channels', 'POST', b));
export const updateChannel = async (id: string, b: ChannelInput) =>
  toAdmin(await send<WireAdminChannel>(`/admin/tv/channels/${enc(id)}`, 'PUT', b));
export const deleteChannel = (id: string) => send<void>(`/admin/tv/channels/${enc(id)}`, 'DELETE');
/** `GET /tv/timezones`: the names the server accepts. */
export const listTimezones = async () => (await get<{ items: string[] }>('/tv/timezones')).items;

/* ------------------------------------------------------ schedule and now */

export interface Preview {
  blocks: TvBlock[];
  warnings: string[];
}
/** Generated from the current rules without storing anything. */
export async function previewSchedule(id: string, fromMs: number, toMs: number): Promise<Preview> {
  const r = await send<{ blocks: WireBlock[]; warnings?: string[] }>(
    `/admin/tv/channels/${enc(id)}/preview`,
    'POST',
    { from_ms: fromMs, to_ms: toMs },
  );
  return { blocks: r.blocks.map(toBlock), warnings: r.warnings ?? [] };
}
/** The blocks stored for the window (what viewers get). */
export async function storedSchedule(id: string, fromMs: number, toMs: number): Promise<TvBlock[]> {
  const r = await get<{ items: WireBlock[] }>(
    `/admin/tv/channels/${enc(id)}/schedule${qs({ from: fromMs, to: toMs })}`,
  );
  return r.items.map(toBlock);
}
export interface Published {
  from_ms: number;
  to_ms: number;
  written: number;
  removed: number;
  warnings: string[];
}
export const publishSchedule = async (id: string, horizonHours: number): Promise<Published> => {
  const r = await send<Published>(`/admin/tv/channels/${enc(id)}/publish`, 'POST', {
    horizon_hours: horizonHours,
  });
  return { ...r, warnings: r.warnings ?? [] };
};

export interface NowState {
  at_ms: number;
  offset_ms: number | null;
  current: TvBlock | null;
  next: TvBlock | null;
  viewers: number;
}
/** What is on at `atMs` (default: now), from stored blocks only: time travel for "simulate now". */
export async function getNowAt(id: string, atMs?: number): Promise<NowState> {
  const r = await get<{
    at_ms: number;
    offset_ms?: number | null;
    current?: WireBlock | null;
    next?: WireBlock | null;
    viewers?: number;
  }>(`/admin/tv/channels/${enc(id)}/now${qs({ at_ms: atMs })}`);
  return {
    at_ms: r.at_ms,
    offset_ms: r.offset_ms ?? null,
    current: r.current ? toBlock(r.current) : null,
    next: r.next ? toBlock(r.next) : null,
    viewers: r.viewers ?? 0,
  };
}
/** Starts that programme now (`offset_ms` into it), cutting the block on air. */
export const putNow = async (
  id: string,
  b: { title_id: string; episode_id?: string | null; offset_ms?: number },
) => toBlock(await send<WireBlock>(`/admin/tv/channels/${enc(id)}/now`, 'PUT', b));
/** Places a programme; whatever it overlaps is trimmed, split or removed. */
export const placeBlock = async (
  id: string,
  b: { title_id: string; episode_id?: string | null; start_ms: number; end_ms?: number },
) => toBlock(await send<WireBlock>(`/admin/tv/channels/${enc(id)}/blocks`, 'POST', b));
export const deleteBlock = (blockId: string) =>
  send<void>(`/admin/tv/blocks/${enc(blockId)}`, 'DELETE');

/* -------------------------------------------------------------- the report */

export interface AiredSummary {
  channel_id: string;
  number: number;
  name: string;
  aired_ms: number;
  programmes: number;
  overrides: number;
}
export interface Report {
  from_ms: number;
  to_ms: number;
  channels: AiredSummary[];
  items: { block: TvBlock; aired_ms: number }[];
}
export async function getReport(fromMs: number, toMs: number, channelId?: string): Promise<Report> {
  const r = await get<{
    from_ms: number;
    to_ms: number;
    channels: AiredSummary[];
    items: { block: WireBlock; aired_ms: number }[];
  }>(`/admin/tv/report${qs({ from: fromMs, to: toMs, channel: channelId })}`);
  return { ...r, items: r.items.map((i) => ({ block: toBlock(i.block), aired_ms: i.aired_ms })) };
}

/** Staff title search for the pickers (the catalog's own admin route). */
export { listTitles } from '../api';

/* ------------------------------------------- weekly grid and guide import */

/** One weekly slot of an imported_guide channel (channel time, 0 = Monday). */
export interface Slot {
  id?: string | null;
  weekday: number;
  /** "HH:MM" in the channel's time zone. */
  start: string;
  duration_min: number;
  show_name: string;
  /** The library title the slot airs; null while it matches nothing (skipped on air). */
  title_id: string | null;
  episode_id?: string | null;
  season_number?: number | null;
  episode_number?: number | null;
  note?: string | null;
}
export const listSlots = async (id: string) =>
  (await get<{ items: Slot[] }>(`/admin/tv/channels/${enc(id)}/slots`)).items;
/** Replaces the whole grid; the server regenerates the schedule that has not aired. */
export const saveSlots = async (id: string, items: Slot[]) =>
  (await send<{ items: Slot[] }>(`/admin/tv/channels/${enc(id)}/slots`, 'PUT', { items })).items;

export type RowStatus = 'matched' | 'ambiguous' | 'missing' | 'invalid';
export interface Candidate {
  title_id: string;
  title: string;
  year: number | null;
  score: number;
}
export interface ReportRow {
  line: number;
  weekday: number;
  start: string;
  duration_min: number;
  show: string;
  season: number | null;
  episode: number | null;
  note: string | null;
  status: RowStatus;
  title_id: string | null;
  candidates: Candidate[];
  message: string | null;
}
export interface GuideReport {
  rows: ReportRow[];
  matched: number;
  ambiguous: number;
  missing: number;
  invalid: number;
}
export interface GuideImport {
  id: string;
  channel_id: string;
  format: 'csv' | 'json';
  confirmed: boolean;
  created_at_ms: number;
  report: GuideReport;
}
/** Uploads the file's text; nothing changes until `confirmImport`. */
export const importGuide = (id: string, format: 'csv' | 'json', content: string) =>
  send<GuideImport>(`/admin/tv/channels/${enc(id)}/imports`, 'POST', { format, content });
/** `choices` picks a title for ambiguous rows (null keeps the slot skipped). */
export const confirmImport = (
  id: string,
  importId: string,
  choices: { line: number; title_id: string | null }[],
) =>
  send<{ slots: number; skipped: number; warnings: string[] }>(
    `/admin/tv/channels/${enc(id)}/imports/${enc(importId)}/confirm`,
    'POST',
    { choices },
  );
/** The template the import expects (a download link; the session cookie authorises it). */
export const templateUrl = (format: 'csv' | 'json') =>
  `/api/v1/admin/tv/guide-template?format=${format}`;
