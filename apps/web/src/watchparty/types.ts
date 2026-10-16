// Wire shapes of the Watch Party API (docs/specs/watch-parties.md section 7). Ids are decimal strings, times are
// unix milliseconds on the SERVER clock. Mirrors crates/sr-watchparty/src/model.rs.

export interface Playback {
  /** Strictly increasing: ignore anything not newer than what was applied. */
  version: number;
  playing: boolean;
  position_ms: number;
  /** Speed multiplier (1 = normal). */
  rate: number;
  /** Server clock at which `position_ms` was true. */
  at_ms: number;
  /** Server clock when this object was built (every state carries it). */
  server_time_ms: number;
  expected_position_ms: number;
  /** Paused by the buffering rule ("Waiting for ..."). */
  auto_paused: boolean;
  duration_ms: number | null;
  live: boolean;
}

export interface PartyMedia {
  title_id: string;
  /** "0" for a movie. */
  episode_id: string;
  variant_id: string | null;
  asset_id: string;
  title: string;
  episode_label: string | null;
  duration_ms: number | null;
  age_restricted: boolean;
  live: boolean;
}

export interface Participant {
  user_id: string;
  username: string;
  display_name: string | null;
  joined_at_ms: number;
  is_host: boolean;
  is_controller: boolean;
  ready: boolean;
  buffering: boolean;
  rtt_ms: number | null;
}

export interface QueueItem extends PartyMedia {
  id: string;
  position: number;
  added_by: string | null;
  added_at_ms: number;
}

export type ControlMode = 'host' | 'controllers' | 'everyone';

export interface PartySettings {
  shared_control: ControlMode;
  queue_policy: ControlMode;
  auto_pause_buffering: boolean;
  max_wait_ms: number;
  persistent: boolean;
  max_participants: number;
}

export type Binding = 'channel' | 'dm' | 'private';

export interface You {
  member: boolean;
  host: boolean;
  can_control: boolean;
  can_queue: boolean;
  can_manage: boolean;
}

export interface Party {
  id: string;
  host_id: string;
  binding: Binding;
  guild_id: string | null;
  channel_id: string | null;
  invite_code: string | null;
  /** `null` for an empty watch party room (state idle, D-227): nothing is playing until its host picks media. */
  media: PartyMedia | null;
  playback: Playback;
  settings: PartySettings;
  participants: Participant[];
  queue: QueueItem[];
  created_at_ms: number;
  ended: boolean;
  you: You | null;
}

export type CommandAction = 'play' | 'pause' | 'seek' | 'rate' | 'ended';

export interface CommandInput {
  action: CommandAction;
  position_ms?: number;
  rate?: number;
  expected_version?: number;
}

/** A Cinema title (`title_id`) or a Live TV channel (`tv_channel_id`, G-16): one of the two. */
export interface CreateInput {
  title_id?: string;
  tv_channel_id?: string | null;
  episode_id?: string | null;
  variant_id?: string | null;
  channel_id?: string | null;
  start_ms?: number;
  settings?: Partial<PartySettings>;
}

export interface HeartbeatInput {
  rtt_ms?: number;
  buffering?: boolean;
  ready?: boolean;
}

export interface HeartbeatResult {
  server_time_ms: number;
  playback: Playback;
  member: boolean;
}

/** `WATCH_PARTY_UPDATE` payloads. */
export type PartyUpdateEvent =
  | { type: 'removed'; party_id: string; reason: string }
  | { type: Exclude<string, 'removed'>; party: Party };

export interface PlaybackEvent {
  party_id: string;
  reason: string;
  by: string | null;
  playback: Playback;
}

export interface ReactionEvent {
  party_id: string;
  user_id: string;
  emoji: string;
  at_ms: number;
}

export interface MessageEvent {
  party_id: string;
  user_id: string;
  content: string;
  at_ms: number;
}
