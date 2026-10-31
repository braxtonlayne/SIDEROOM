// FND-API-008: Discord-shaped entities held by the normalized store.
// Field names are snake_case to match the wire format (docs/05). IDs are snowflake strings.

export interface User {
  id: string;
  username: string;
  global_name?: string | null;
  avatar?: string | null;
  bot?: boolean;
  /** Patron badge and ring show for this person (D-256); false when hidden or not a Patron. */
  sr_patron?: boolean;
  /** Webhook authors only: the avatar of this message, an image-proxy path (DEV-BOT-006). */
  avatar_url?: string | null;
  /** The server tag this person displays (LNG-SSET-012); the API sends it only while the server allows it. */
  primary_guild?: {
    identity_guild_id?: string | null;
    identity_enabled?: boolean;
    tag?: string | null;
    badge?: number | null;
    color?: number | null;
  } | null;
}

export interface Guild {
  id: string;
  name: string;
  icon?: string | null;
  /** Banner hash, served from the same CDN kind as the icon (LNG-GLD-019). */
  banner?: string | null;
  owner_id?: string;
  member_count?: number;
  /** True for READY stubs until GUILD_CREATE arrives. */
  unavailable?: boolean;
  /** The Staff Server (ADM-STF-010). Sent by the API when the viewer is a member. */
  is_staff_guild?: boolean;
  /** Community channels (Discord shape, null when unset; G-76). */
  rules_channel_id?: string | null;
  public_updates_channel_id?: string | null;
  safety_alerts_channel_id?: string | null;
  /** Server Protection (D-224): links whose text names another host show their raw target and a warning. */
  hide_suspicious_links?: boolean;
  /** Timed invite and DM pauses and the last join-rate alert (D-243); a time in the past means it ended. */
  incidents_data?: {
    invites_disabled_until: string | null;
    dms_disabled_until: string | null;
    raid_detected_at: string | null;
  };
  /** Largest file the viewer may upload in this server, in bytes (D-233); absent until `GET /guilds/{id}` is read. */
  sr_max_upload_bytes?: number;
  /** Side Room's profile extras: the server's traits (J-07). */
  sr_profile?: {
    traits?: { emoji_id?: string | null; emoji_name?: string | null; label: string }[];
  };
}

export interface Channel {
  id: string;
  /** Discord channel type (0 text, 1 DM, 2 voice, 4 category, 5 announcement, ...). */
  type: number;
  guild_id?: string;
  name?: string | null;
  topic?: string | null;
  parent_id?: string | null;
  position?: number;
  last_message_id?: string | null;
  /** Slowmode seconds. */
  rate_limit_per_user?: number;
  /** DM / group DM participants. */
  recipients?: User[];
  /** Group DM owner (contract: Discord's `owner_id`). */
  owner_id?: string | null;
  /** Group DM icon hash, served from `/cdn/icons/{channel_id}/{hash}` (contract). */
  icon?: string | null;
  /** DM from a non-friend awaiting acceptance; such channels stay out of the DM list. */
  is_message_request?: boolean | null;
  /** Voice channels: maximum simultaneous participants (0 or absent = unlimited). */
  user_limit?: number;
  /** Watch party room (D-208): a voice channel with a standing party. Absent when the server does not report it. */
  sr_watch_party_room?: boolean;
  /** Voice channels: audio bitrate in bits per second. */
  bitrate?: number;
  /** Voice channels: members with Speak may play music into the call (MUS-JB-001; absent means allowed). */
  sr_music_allowed?: boolean;
  /** Voice channels: the music track's bitrate in kbit/s (32 to 320, default 256). */
  sr_music_bitrate_kbps?: number;
  /** The Discord bridge mirrors this channel (BRG-UI-001); absent when it is not mapped. */
  sr_bridge?: { direction: 'both' | 'to_discord' | 'from_discord' };
  /** Age-restricted channel (the Lounge asks the viewer to confirm they are 18 or older). */
  nsfw?: boolean;
  /** Per-channel role/member overwrites (type 0 role, 1 member; allow/deny are decimal strings). */
  permission_overwrites?: { id: string; type: number; allow: string; deny: string }[];
  /** DM and group DM channels: the largest file the viewer may upload there, in bytes (D-233). */
  sr_max_upload_bytes?: number;
}

export interface Role {
  id: string;
  guild_id: string;
  name: string;
  color: number;
  position: number;
  /** Permission bitfield as a decimal string. */
  permissions: string;
  hoist?: boolean;
  /** Role icon emoji (shown next to the name in messages). */
  unicode_emoji?: string | null;
  /** Gradient stops (integers); `primary_color` mirrors `color` (J-08). */
  colors?: {
    primary_color?: number;
    secondary_color?: number | null;
    tertiary_color?: number | null;
  };
}

export interface Member {
  guild_id: string;
  user_id: string;
  nick?: string | null;
  roles: string[];
  joined_at?: string;
  /** ISO end of the member's timeout, when timed out. */
  communication_disabled_until?: string | null;
}

export interface Message {
  id: string;
  channel_id: string;
  guild_id?: string;
  author: User;
  content: string;
  timestamp: string;
  edited_timestamp?: string | null;
  /** Set when the message was posted through a webhook; `author` is then the webhook (DEV-BOT-006). */
  webhook_id?: string;
  /**
   * Set on a message the Discord bridge brought in (BRG-GW-001): who wrote it on Discord. `author_avatar` is a
   * same-origin `/cdn/external/...` path (never a Discord URL). The Lounge shows a DISCORD tag beside the name.
   */
  sr_bridge?: { platform: 'discord'; author_name: string; author_avatar?: string | null };
  /** Application emoji ids in `content` that stay text because the author is not that application's bot (D-237). */
  sr_text_emoji?: string[];
  pinned?: boolean;
  nonce?: string | number | null;
  mentions?: User[];
  mention_everyone?: boolean;
  mention_roles?: string[];
  /** `MESSAGE_CREATE` of a message a moderator restored from a case (D-338): not new, no badge or alert. */
  restored?: boolean;
  message_reference?: {
    message_id: string;
    channel_id?: string;
    guild_id?: string;
    /** 0 reply, 1 forward. */
    type?: number;
  } | null;
  reactions?: MessageReaction[];
  /** 0 default, 6 pin notice, 19 reply (Discord message types). */
  type?: number;
  /** 4 suppress embeds, 8192 voice message, 1 << 30 written by the system user (SYSTEM_TAG), 1 << 29 flagged as likely spam (LIKELY_SPAM). */
  flags?: number;
  attachments?: RawAttachment[];
  embeds?: RawEmbed[];
  sticker_items?: { id: string; name: string; format_type: number }[];
  poll?: RawPoll | null;
  components?: RawComponent[];
  message_snapshots?: { message: Partial<Message> }[];
  /** Text-to-speech message (needs SEND_TTS_MESSAGES). */
  tts?: boolean;
  /** The thread started from this message (its id is the message id). */
  thread?: { id: string; name?: string | null; message_count?: number | null } | null;
  /** Client-only: set while the message is optimistic. Absent on server messages. */
  local?: LocalState;
}

/** Message components as stored (Discord shape). Interactions are not supported by the server yet. */
export interface RawComponent {
  type: number;
  style?: number;
  label?: string;
  url?: string;
  disabled?: boolean;
  custom_id?: string;
  placeholder?: string;
  options?: { label: string; value: string; description?: string }[];
  components?: RawComponent[];
}

export interface RawAttachment {
  id: string;
  filename: string;
  size: number;
  url: string;
  content_type?: string | null;
  description?: string | null;
  width?: number | null;
  height?: number | null;
  duration_secs?: number | null;
  /** Base64 amplitude bytes (voice messages). */
  waveform?: string | null;
  /** BlurHash placeholder (images), declared by the uploader. */
  blurhash?: string | null;
  expires_at?: string | null;
  /** The viewer's sensitive-media setting withheld the file; `url` is empty (D-230). */
  sr_filtered?: boolean;
}

export interface RawEmbed {
  type?: string;
  title?: string;
  description?: string;
  url?: string;
  color?: number;
  timestamp?: string;
  footer?: { text: string; icon_url?: string; proxy_icon_url?: string };
  /**
   * `proxy_url` (a same-origin `/cdn/external/...` path, D-176) is the only form of a third-party
   * image the client loads without a click; `url` is the original on the other site.
   */
  image?: { url?: string; proxy_url?: string; width?: number; height?: number };
  thumbnail?: { url?: string; proxy_url?: string; width?: number; height?: number };
  video?: { url?: string; proxy_url?: string; width?: number; height?: number };
  provider?: { name?: string; url?: string };
  author?: { name: string; url?: string; icon_url?: string; proxy_icon_url?: string };
  fields?: { name: string; value: string; inline?: boolean }[];
  /** Set by the server on a preview of a link the author wrote inside a spoiler (LNG-MSG-015). */
  sr_spoiler?: boolean;
  /**
   * Set by the server on a link preview with a "Play here" offer (PRF-UNF-001): the provider's privacy-enhanced
   * embed (`mode: 'embed'`) or a direct media file (`mode: 'direct'`). The client loads `src` only after a click.
   */
  sr_play?: {
    mode: 'embed' | 'direct';
    provider: string;
    src: string;
    media?: 'video' | 'audio' | 'hls';
    /** Watch Together can drive this player. */
    sync?: boolean;
  };
}

export interface RawPoll {
  question: { text?: string };
  answers: {
    answer_id: number;
    poll_media: { text?: string; emoji?: { id?: string | null; name?: string | null } };
  }[];
  expiry?: string | null;
  allow_multiselect?: boolean;
  results?: {
    is_finalized: boolean;
    answer_counts: { id: number; count: number; me_voted: boolean }[];
  };
}

export interface MessageReaction {
  emoji: { id?: string | null; name?: string | null };
  count: number;
  /** The signed-in user reacted with this emoji. */
  me: boolean;
}

/** Optimistic-send state. `sending` -> replaced by the server message; `failed` stays until retry/discard. */
export interface LocalState {
  status: 'sending' | 'failed';
  nonce: string;
  error?: string;
}

export type PresenceStatus = 'online' | 'idle' | 'dnd' | 'offline';

export interface Presence {
  user_id: string;
  status: PresenceStatus;
  activities: unknown[];
}

export interface ReadState {
  channel_id: string;
  last_message_id: string | null;
  mention_count: number;
}

export interface TypingEntry {
  user_id: string;
  /** Epoch ms after which the entry is stale (Discord shows ~10 s). */
  expires_at: number;
}

/** Ordered window of loaded messages for one channel (ascending snowflake order). */
export interface MessageWindow {
  ids: string[];
  /** Optimistic messages (ids are `pending:<nonce>`), oldest first, rendered after `ids`. */
  pending: string[];
  /** Older history exists on the server that is not loaded. */
  hasMoreBefore: boolean;
  /** `ids` ends at the newest message, so live MESSAGE_CREATE can be appended. */
  atLatest: boolean;
  loading: boolean;
  error: string | null;
  /**
   * Kept from an earlier session by READY (K-22): shown at once, but events may have been missed, so the
   * newest page is fetched again and replaces it.
   */
  stale?: boolean;
}

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed';

/** The synced per-user settings blob (`/users/@me/settings`): a client-owned JSON object plus its version. */
export interface UserSettingsBlob {
  version: number;
  settings: Record<string, unknown>;
}

/**
 * A user's voice connection (Discord `VoiceState`). `channel_id: null` means "not connected" and the
 * reducer drops the entry. Keyed by guild id (`@me` for DM calls) then user id in the store.
 */
export interface VoiceState {
  guild_id?: string | null;
  channel_id: string | null;
  user_id: string;
  session_id?: string;
  /** Server deafen / mute (moderators). */
  deaf?: boolean;
  mute?: boolean;
  self_deaf?: boolean;
  self_mute?: boolean;
  self_video?: boolean;
  self_stream?: boolean;
  /** Stage channels: suppressed = audience. */
  suppress?: boolean;
  /** Stage channels: when the member raised their hand (ISO 8601), null when not asking to speak. */
  request_to_speak_timestamp?: string | null;
}

/** An ICE server as delivered in `VOICE_SERVER_UPDATE` (RTCIceServer shape). */
export interface VoiceIceServer {
  urls: string | string[];
  username?: string;
  credential?: string;
}

/** `VOICE_SERVER_UPDATE`: where and how to open the voice WebSocket (docs/specs/lounge-voice-video.md section 4). */
export interface VoiceServerInfo {
  guild_id: string | null;
  channel_id?: string | null;
  /** Host (and optional port) of the `sr-voice` WebSocket. */
  endpoint: string;
  /** Short-lived token for the voice IDENTIFY. */
  token: string;
  ice_servers?: VoiceIceServer[];
}
