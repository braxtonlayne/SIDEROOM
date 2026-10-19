// Live TV wire shapes as the web UI uses them (spec cinema-live-tv sections 2 and 5). The backend (p8-livetv-be)
// owns the contract; everything the UI knows about the HTTP API is in api.ts, so a difference is a one-file change.
// Timestamps are Unix milliseconds, ids are Snowflake strings.

export type ChannelKind = 'imported_guide' | 'rule_based' | 'manual';
export type ChannelCategory = 'anime' | 'movies' | 'series' | 'custom';
export type ProgramKind = 'movie' | 'series' | 'anime' | 'filler';

export interface TvChannel {
  id: string;
  /** Channel number (101, 102, ...). */
  number: number;
  /** Call sign shown as the logo text when there is no logo image: "[adult swim]". */
  call_sign: string;
  name: string;
  /** Sub-label under the call sign: the network, or "CUSTOM". */
  network?: string | null;
  era_label?: string | null;
  logo_url?: string | null;
  kind: ChannelKind;
  category?: ChannelCategory | null;
  /** IANA zone the channel schedule is written in (D-103). */
  timezone: string;
  age_restricted: boolean;
  /** Distinct members who fetched a live playlist in the last 45 s. */
  viewers?: number;
}

export interface TvBlock {
  id: string;
  channel_id: string;
  start_ms: number;
  end_ms: number;
  title: string;
  /** "S1 · E3 · The Long Night". */
  episode_label?: string | null;
  kind: ProgramKind;
  title_id?: string | null;
  episode_id?: string | null;
  year?: number | null;
  runtime_min?: number | null;
  content_rating?: string | null;
  age_restricted?: boolean;
  synopsis?: string | null;
  poster_url?: string | null;
  backdrop_url?: string | null;
  /** False for slates and off-air blocks (no stream exists for them). */
  playable?: boolean;
  source?: 'generated' | 'manual' | 'override';
}

export interface TvGuide {
  now_ms: number;
  channels: TvChannel[];
  /** Blocks per channel id, sorted by start. */
  blocks: Record<string, TvBlock[]>;
  /** Operator-editable footer note (empty means the default text). */
  footer_note?: string | null;
}

export interface ChannelNow {
  now: TvBlock | null;
  next: TvBlock | null;
  /** The stream is healthy ("GOOD RECEPTION"). */
  healthy: boolean;
  server_time_ms: number;
  /** How far into the current programme the wall clock is. */
  offset_ms?: number | null;
  viewers?: number;
}

/** Everything the live player needs to tune a channel. */
export interface TuneGrant {
  channel: TvChannel;
  /** Signed HLS master playlist (live). */
  master_url: string;
  /** The channel is off air and `master_url` plays the instance slate (D-266). */
  slate?: boolean;
  expires_at_ms: number;
  refresh_at_ms?: number | null;
  now: TvBlock | null;
  next: TvBlock | null;
  server_time_ms: number;
  /** Seconds of rewind the playlists offer. Absent or small means no DVR window. */
  dvr_window_s?: number | null;
}

export const FILLER_POLICIES = ['bumpers', 'next_program_early', 'slate', 'off_air'] as const;
export type FillerPolicy = (typeof FILLER_POLICIES)[number];

export const CHANNEL_KINDS: { value: ChannelKind; label: string }[] = [
  { value: 'rule_based', label: 'Rule based' },
  { value: 'manual', label: 'Manual calendar' },
  { value: 'imported_guide', label: 'Imported guide' },
];
