// Types for the developer platform HTTP API, following docs/specs/developer-platform.md section 3 ("HTTP API as
// built", feat/p8-dev-be). Snowflakes and permission bitfields travel as strings.

export type IntentName = 'GUILD_MEMBERS' | 'GUILD_PRESENCES' | 'MESSAGE_CONTENT';
export type ReleaseState = 'development' | 'in_review' | 'released' | 'suspended';

export interface BotUser {
  id: string;
  username: string;
  global_name: string | null;
  /** `disabled` after 25 consecutive failed deliveries within 10 minutes (D-186); owner view, with a URL. */
  interactions_endpoint_state?: 'enabled' | 'disabled';
  avatar: string | null;
  bot: true;
}

export interface DevApp {
  /** Also the OAuth2 client id. */
  id: string;
  name: string;
  description: string;
  /** CDN hash of the app icon. */
  icon: string | null;
  tags: string[];
  bot_public: boolean;
  bot_require_code_grant: boolean;
  flags: number;
  release_state: ReleaseState;
  owner: { id: string; username: string; global_name: string | null };
  /** Every application has a bot account (created together with it). */
  bot: BotUser;
  redirect_uris: string[];
  install_params: { scopes: string[]; permissions: string };
  terms_of_service_url: string | null;
  privacy_policy_url: string | null;
  privileged_intents: { requested: IntentName[]; approved: IntentName[] };
  /** Last characters of the client secret, e.g. `...abcd`; the secret itself is never returned again. */
  client_secret_hint: string | null;
  /** HTTPS endpoint that receives signed interactions instead of the gateway (owner view). */
  interactions_endpoint_url?: string | null;
  /** Ed25519 public key (hex) that verifies those requests; absent until created. */
  verify_key?: string | null;
  /** `disabled` after 25 consecutive failed deliveries within 10 minutes (D-186); owner view, with a URL. */
  interactions_endpoint_state?: 'enabled' | 'disabled';
  created_at: string;
}

export interface AppPatch {
  name?: string;
  description?: string;
  /** `data:` URI to set, null to clear. */
  icon?: string | null;
  bot_public?: boolean;
  redirect_uris?: string[];
  install_params?: { scopes: string[]; permissions: string };
  privacy_policy_url?: string | null;
  terms_of_service_url?: string | null;
  /** The privileged intents the bot asks for (the full requested set). */
  privileged_intents?: IntentName[];
  /** Saving verifies it with a signed PING; null removes it. */
  interactions_endpoint_url?: string | null;
}

/** Returned once by POST /applications. */
export interface AppCreated {
  application: DevApp;
  client_secret: string;
  bot_token: string;
}

export interface DeveloperRequest {
  id: string;
  account_id: string;
  state: 'pending' | 'approved' | 'rejected' | 'suspended';
  pitch: string;
  prior_work: string;
  created_at: string;
  reviewed_at: string | null;
  reviewer_note: string | null;
}

export interface DeveloperStatus {
  is_developer: boolean;
  request: DeveloperRequest | null;
}

export interface ApplyBody {
  pitch: string;
  prior_work?: string;
  accepted_terms: true;
}

export type VersionState = 'pending' | 'approved' | 'rejected' | 'changes_requested' | 'withdrawn';

export interface AppVersion {
  id: string;
  application_id: string;
  version: number;
  state: VersionState;
  needs_review: boolean;
  needs_review_reasons: ('scopes' | 'redirect_uris' | 'intents' | 'first_release')[];
  diff: { changed: { key: string; old: unknown; new: unknown }[] };
  note: string | null;
  reviewer_note: string | null;
  created_at: string;
  decided_at: string | null;
}

export interface AppInstall {
  guild_id: string;
  installed_by: string;
  scopes: string[];
  permissions: string;
  created_at: string;
}

/** Slash command fields the builder produces (see spec 3.5; commands are managed with the bot token). */
export const OPTION_TYPES = [
  { value: 3, label: 'String' },
  { value: 4, label: 'Integer' },
  { value: 5, label: 'Boolean' },
  { value: 6, label: 'User' },
  { value: 7, label: 'Channel' },
  { value: 8, label: 'Role' },
  { value: 9, label: 'Mentionable' },
  { value: 10, label: 'Number' },
  { value: 11, label: 'Attachment' },
] as const;

export interface CommandOption {
  type: number;
  name: string;
  description: string;
  required: boolean;
  autocomplete?: boolean;
}

export type CommandType = 1 | 2 | 3; // chat input, user, message

export interface CommandInput {
  type: CommandType;
  name: string;
  description: string;
  options: CommandOption[];
}

export interface ConsentScope {
  name: string;
  description: string;
}

export interface ConsentGuild {
  id: string;
  name: string;
  icon: string | null;
  owner: boolean;
  permissions: string;
  can_install: boolean;
  reason?: 'missing_permission' | 'app_not_released' | 'bot_not_public';
}

/** GET /oauth2/authorize: everything the consent screen shows. */
export interface AuthorizePreview {
  application: {
    id: string;
    name: string;
    description: string;
    icon: string | null;
    bot_public: boolean;
    release_state: ReleaseState;
    privacy_policy_url: string | null;
    terms_of_service_url: string | null;
  };
  bot: BotUser | null;
  scopes: ConsentScope[];
  redirect_uri: string | null;
  state: string | null;
  user: { id: string; username: string; global_name: string | null };
  /** Filled only when the request has the bot or applications.commands scope. */
  guilds: ConsentGuild[];
  /** Permission bitfield the bot asks for (decimal string). */
  permissions: string;
  already_authorized: boolean;
}

export interface AuthorizeResult {
  /** Where to send the browser next: the redirect URI with `code`, or `/oauth2/authorized`. */
  location: string;
  already_installed?: boolean;
}

export interface OAuthGrant {
  id: string;
  application: { id: string; name: string; icon: string | null };
  scopes: string[];
  created_at: string;
}

/** An application tester (D-204): invited accounts answer a system DM; accepted ones may install a development app. */
export interface AppTester {
  /** Invitation id. */
  id: string;
  user_id: string;
  username: string | null;
  global_name: string | null;
  state: 'invited' | 'accepted';
  created_at: string;
}

/** An emoji owned by an application (Emojis tab). `url` is a short-lived signed CDN path. */
export interface AppEmoji {
  id: string;
  name: string;
  animated: boolean;
  url: string;
  created_at: string;
}

/** A Rich Presence art asset: 1 large image, 2 small image. */
export interface AppAsset {
  id: string;
  name: string;
  type: 1 | 2;
  url: string;
}

/** One URL mapping of an Activity (DEV-PORT-003 Activities tab). */
export interface ActivityMapping {
  prefix: string;
  target: string;
}

export type ActivityReviewState = 'draft' | 'pending' | 'approved' | 'rejected';

export type BundleState = ActivityReviewState | 'superseded';

/** One uploaded Activity bundle (a zip of static files, J-12). */
export interface ActivityBundle {
  id: string;
  /** SHA-256 (hex) of the uploaded zip: what staff approve. */
  sha256: string;
  /** Total unpacked size in bytes. */
  byte_size: number;
  entry_count: number;
  state: BundleState;
  uploaded_at_ms: number;
  reviewed_at_ms: number | null;
  note: string | null;
}

export interface ActivityBundleEntry {
  path: string;
  size: number;
  content_type: string;
}

/** `GET /applications/{id}/activity/bundles/{bundle}`. */
export interface ActivityBundleDetail {
  bundle: ActivityBundle;
  entries: ActivityBundleEntry[];
}

/** `GET /applications/{id}/activity`. */
export interface ActivityConfig {
  config: {
    application_id: string;
    /** Live URL served only while the application is in development; never reviewed. */
    dev_entry_url: string | null;
    url_mappings: ActivityMapping[];
    review_state: ActivityReviewState;
    /** The bundle staff approved; the only one served. */
    reviewed_bundle: ActivityBundle | null;
    /** Recent bundles, newest first. */
    bundles: ActivityBundle[];
    review_note: string | null;
    updated_at_ms: number;
  };
  /** The server runs Activities (`[activities] enabled`). */
  enabled: boolean;
  /** `https://<application-id>.activities.<host>`. */
  origin: string;
  /** `<origin>/`: a redirect URI of the application without being stored (J-13). */
  implicit_redirect_uri: string | null;
}
