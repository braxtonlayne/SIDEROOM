// REST helpers for the Admin pages: /api/v1/admin/*. Shapes mirror crates/sr-admin (utoipa schemas).
// Ids are snowflake strings. Mutations never retry on 429 so the staff member sees the error.
import { ApiError, apiFetch } from '../api/client';
import { withReauth } from './reauth';

export { errorText } from '../settings/server/api';

// Every admin call goes through withReauth: guarded actions answer 403 900004 until the staff member has
// proved a second factor (ADM-STF-009), and the retry happens after the re-auth dialog.
const send = <T>(path: string, method: string, json?: unknown) =>
  withReauth(() =>
    apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) }),
  );
const get = <T>(path: string) => withReauth(() => apiFetch<T>(path, { retries: 0 }));

const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

export type AccountState = 'active' | 'deactivated' | 'deleted' | 'platform_banned';
export type BanCategory = 'serious' | 'standard';

export interface Me {
  is_owner: boolean;
  platform_permissions: string;
  user_id: string;
}
export const getMe = () => get<Me>('/admin/me');

// ---- stats and system ----

export interface SystemCounts {
  accounts_active: number;
  accounts_banned: number;
  accounts_deactivated: number;
  accounts_deleted: number;
  jobs_dead: number;
  jobs_queued: number;
  jobs_running: number;
  sessions_active: number;
  staff: number;
  /** Added by the phase-10 admin backend; older servers omit them. */
  accounts_cap?: number;
  alerts_unacked?: number;
  invites_open?: number;
  review_pending?: number;
  storage_bytes?: number;
  /** Not served yet (no voice stats route): shown only when a server sends them. */
  voice_rooms?: number;
  egress_bytes?: number;
}
export const getStats = () => get<SystemCounts>('/admin/stats');

export interface Probe {
  name: string;
  ok: boolean;
  error?: string | null;
}
/** Whether an optional integration is set up; never carries a secret (ADM-DASH-003). */
export interface Integration {
  id: string;
  configured: boolean;
  detail?: string | null;
}
/** Backup times read from the backup folder (null when never run), K-10. */
export interface BackupsView {
  last_run_ms: number | null;
  last_verify_ms: number | null;
  snapshot_ms: number | null;
}
/** One folder's space; sizes are null when the path cannot be read. */
export interface DiskView {
  path: string;
  total_bytes: number | null;
  free_bytes: number | null;
}

export interface SystemView {
  counts: SystemCounts;
  /** Added with K-10; older servers omit them. */
  backups?: BackupsView;
  disk?: DiskView[];
  integrations?: Integration[];
  gateway_sessions?: number | null;
  probes: Probe[];
  version: string;
}
export const getSystem = () => get<SystemView>('/admin/system');
/** Sends a test message to the caller's own address (OPERATOR, fresh second factor). */
export const sendEmailTest = () => send<{ sent_to: string }>('/admin/system/email-test', 'POST');

export interface JobRow {
  id: string;
  kind: string;
  state: string;
  attempts: number;
  max_attempts: number;
  run_at_ms: number;
  finished_at_ms?: number | null;
  last_error?: string | null;
}
export interface JobPage {
  items: JobRow[];
  next_after?: string | null;
}
export const listJobs = (p: { state?: string; after?: string | null; limit?: number } = {}) =>
  get<JobPage>(`/admin/jobs${qs(p)}`);
export const retryJob = (id: string, reason?: string) =>
  send<void>(`/admin/jobs/${id}/retry`, 'POST', { reason: reason || null });
export const deleteJob = (id: string) => send<void>(`/admin/jobs/${id}`, 'DELETE');

// ---- accounts ----

export interface AccountSummary {
  id: string;
  person_id: string;
  username: string;
  display_name?: string | null;
  email: string;
  email_verified: boolean;
  state: AccountState;
  state_changed_at_ms: number;
  created_at_ms: number;
  flags: number;
}
export interface AccountPage {
  items: AccountSummary[];
  next_after?: string | null;
}
export interface AccountRef {
  id: string;
  username: string;
}
export interface LoginEvent {
  at_ms: number;
  ip?: string | null;
  success: boolean;
}
export interface AccountDetail {
  account: AccountSummary;
  credit_balance: number;
  invited_by?: AccountRef | null;
  /** Staff froze this account's invite creation (ADM-GRAPH-004). */
  invites_frozen?: boolean;
  invites_issued: number;
  invites_redeemed: number;
  mfa_enabled: boolean;
  person_banned: boolean;
  recent_logins: LoginEvent[];
  sessions_active: number;
  suspicion_level: number;
}
export const searchAccounts = (p: {
  q?: string;
  /** Exact Discord user id (ADM-PPL-001). */
  discord_id?: string;
  /** An IP address seen in sign-ins; the lookup is written to the access log. */
  ip?: string;
  state?: string;
  after?: string | null;
  limit?: number;
}) => get<AccountPage>(`/admin/accounts${qs(p)}`);
export const getAccount = (id: string) => get<AccountDetail>(`/admin/accounts/${id}`);
const reasonBody = (reason: string) => ({ reason });
export const suspendAccount = (id: string, reason: string) =>
  send<void>(`/admin/accounts/${id}/suspend`, 'POST', reasonBody(reason));
export const unsuspendAccount = (id: string, reason: string) =>
  send<void>(`/admin/accounts/${id}/unsuspend`, 'POST', reasonBody(reason));
export const forceLogout = (id: string, reason: string) =>
  send<{ sessions_revoked: number }>(
    `/admin/accounts/${id}/force-logout`,
    'POST',
    reasonBody(reason),
  );
export interface Reset2faReport {
  totp: number;
  recovery_codes: number;
  passkeys: number;
  trusted_devices: number;
  sessions_revoked: number;
}
export const reset2fa = (id: string, reason: string) =>
  send<Reset2faReport>(`/admin/accounts/${id}/reset-2fa`, 'POST', reasonBody(reason));
export const renameAccount = (
  id: string,
  body: { username?: string | null; display_name?: string | null },
  reason: string,
) => send<AccountSummary>(`/admin/accounts/${id}/rename`, 'POST', { ...body, reason });
export const changeAccountEmail = (id: string, email: string, reason: string) =>
  send<AccountSummary>(`/admin/accounts/${id}/change-email`, 'POST', { email, reason });
/** Badge names staff may grant (`sr_admin::BADGES`); bits 8 to 11 of `accounts.flags`. */
export const STAFF_BADGES = [
  { name: 'early_member', bit: 8 },
  { name: 'contributor', bit: 9 },
  { name: 'bug_hunter', bit: 10 },
  { name: 'honorary', bit: 11 },
] as const;
export const setAccountBadges = (id: string, grant: string[], revoke: string[], reason: string) =>
  send<AccountSummary>(`/admin/accounts/${id}/badges`, 'POST', { grant, revoke, reason });
export const forcePasswordReset = (id: string, reason: string) =>
  send<{ sessions_revoked: number }>(
    `/admin/accounts/${id}/force-password-reset`,
    'POST',
    reasonBody(reason),
  );
/** Why an account is banned (D-211); each has a default category the dialog starts from. */
export type BanReason =
  'illegal_content' | 'child_safety' | 'fraud' | 'platform_abuse' | 'harassment' | 'ban_evasion';
export const BAN_REASONS: { value: BanReason; serious: boolean }[] = [
  { value: 'illegal_content', serious: true },
  { value: 'child_safety', serious: true },
  { value: 'fraud', serious: true },
  { value: 'platform_abuse', serious: true },
  { value: 'harassment', serious: false },
  { value: 'ban_evasion', serious: false },
];
export const banAccount = (
  id: string,
  category: BanCategory,
  reason: string,
  reason_category?: BanReason,
) => send<BanOutcome>(`/admin/accounts/${id}/ban`, 'POST', { category, reason, reason_category });

// ---- persons and bans ----

export interface BanRecord {
  id: string;
  person_id: string;
  banned_by: string;
  category: BanCategory;
  note: string;
  created_at_ms: number;
  lifted_at_ms?: number | null;
}
export interface BanPage {
  items: BanRecord[];
  next_after?: string | null;
}
export interface SuspicionChange {
  escalated: boolean;
  frozen: boolean;
  inviter_account_id: string;
  level: number;
  person_id: string;
}
export interface BanOutcome {
  accounts_banned: number;
  ban: BanRecord;
  inviter_suspicion?: SuspicionChange | null;
  sessions_revoked: number;
}
export interface UnbanOutcome {
  accounts_left_deactivated: number;
  accounts_reactivated: number;
}
/** A Discord identity of one of a Person's accounts; a moved one is history (D-262). */
export interface PersonIdentity {
  id: string;
  account_id: string;
  provider: string;
  provider_user_id: string;
  linked_at_ms: number;
  unlinked_at_ms?: number | null;
  moved_to_account_id?: string | null;
}
export interface PersonDetail {
  id: string;
  accounts: AccountSummary[];
  identities?: PersonIdentity[];
  ban?: BanRecord | null;
  notes: string;
  suspicion_level: number;
}
export const listBans = (
  p: { active_only?: boolean; after?: string | null; limit?: number } = {},
) => get<BanPage>(`/admin/bans${qs(p)}`);
export const getPerson = (id: string) => get<PersonDetail>(`/admin/persons/${id}`);
export const unbanPerson = (id: string, reason: string) =>
  send<UnbanOutcome>(`/admin/persons/${id}/unban`, 'POST', reasonBody(reason));

// ---- invites, credits, suspicion ----

export interface InviteRow {
  id: string;
  code: string;
  issuer_account_id: string;
  state: string;
  emergency: boolean;
  note: string;
  created_at_ms: number;
  redeemed_by_account_id?: string | null;
}
export interface InvitePage {
  items: InviteRow[];
  next_after?: string | null;
}
export const listInvites = (p: {
  issuer_id?: string;
  state?: string;
  after?: string | null;
  limit?: number;
}) => get<InvitePage>(`/admin/invites${qs(p)}`);
export const revokeInvite = (id: string) => send<void>(`/admin/invites/${id}`, 'DELETE');

export interface LedgerEntry {
  id: string;
  delta: number;
  reason: string;
  created_at_ms: number;
  ref_id?: string | null;
}
export interface CreditLedger {
  balance: number;
  entries: LedgerEntry[];
}
export const getCredits = (accountId: string) =>
  get<CreditLedger>(`/admin/accounts/${accountId}/credits`);
export const adjustCredits = (accountId: string, delta: number, reason: string) =>
  send<{ balance: number }>(`/admin/accounts/${accountId}/credits`, 'POST', { delta, reason });

export interface InviteNode {
  account_id: string;
  depth: number;
  parent_id?: string | null;
  person_id: string;
  state: AccountState;
  suspicion_level: number;
  username: string;
}
export interface InviteStats {
  banned_descendants: number;
  descendants: number;
  issued: number;
  open: number;
  redeemed: number;
  revoked: number;
}
export interface InviteTree {
  account: InviteNode;
  ancestors: InviteNode[];
  stats: InviteStats;
}
export const getInviteTree = (accountId: string) =>
  get<InviteTree>(`/admin/accounts/${accountId}/invite-tree`);

export interface SuspectPerson {
  accounts: AccountRef[];
  frozen: boolean;
  person_id: string;
  suspicion_level: number;
}
export const listSuspects = () => get<SuspectPerson[]>('/admin/suspicion');
export const clearSuspicion = (personId: string, reason: string) =>
  send<void>(`/admin/persons/${personId}/clear-suspicion`, 'POST', reasonBody(reason));

// ---- staff ----

export interface StaffRole {
  id: string;
  name: string;
  platform_permissions: string;
  position: number;
}
export interface StaffMember {
  account_id: string;
  username: string;
  display_name?: string | null;
  appointed_at_ms: number;
  appointed_by?: string | null;
  is_owner: boolean;
  platform_permissions: string;
  reason: string;
  roles: StaffRole[];
}
export const listStaff = () => get<StaffMember[]>('/admin/staff');
export interface FormerStaff {
  account_id: string;
  username: string;
  revoked_reason: 'left' | 'kicked' | 'revoked';
  revoked_at_ms: number;
  note: string;
}
export const listFormerStaff = () => get<FormerStaff[]>('/admin/staff/former');
export const listStaffRoles = () => get<StaffRole[]>('/admin/staff/roles');
export const appoint = (account_id: string, role_ids: string[], reason: string) =>
  send<StaffMember>('/admin/staff', 'POST', { account_id, role_ids, reason });
export const setStaffRoles = (accountId: string, role_ids: string[], reason: string) =>
  send<StaffMember>(`/admin/staff/${accountId}/roles`, 'PUT', { role_ids, reason });

export type OfferState = 'pending' | 'accepted' | 'declined' | 'cancelled' | 'expired';
export interface StaffOffer {
  id: string;
  account_id: string;
  username: string;
  offered_by: string;
  offered_by_name: string;
  role_ids: string[];
  role_names: string[];
  reason: string;
  state: OfferState;
  created_at_ms: number;
  expires_at_ms: number;
  responded_at_ms?: number | null;
}
export const listStaffOffers = () => get<StaffOffer[]>('/admin/staff/offers');
export const sendStaffOffer = (account_id: string, role_ids: string[], reason: string) =>
  send<StaffOffer>('/admin/staff/offers', 'POST', { account_id, role_ids, reason });
export const withdrawStaffOffer = (id: string) =>
  send<StaffOffer>(`/admin/staff/offers/${id}`, 'DELETE');

export interface Delegation {
  account_id: string;
  username: string;
  max_permissions: string;
  granted_by: string;
  granted_at_ms: number;
  reason: string;
}
export const listDelegations = () => get<Delegation[]>('/admin/staff/delegations');
export const setDelegation = (accountId: string, max_permissions: string, reason: string) =>
  send<Delegation>(`/admin/staff/${accountId}/delegation`, 'PUT', { max_permissions, reason });
export const removeDelegation = (accountId: string) =>
  send<void>(`/admin/staff/${accountId}/delegation`, 'DELETE');

// ---- platform announcements (ADM-PLT-009) ----

export type AnnouncementSeverity = 'info' | 'warning' | 'critical';
export interface PlatformAnnouncement {
  id: string;
  title: string;
  body: string;
  severity: AnnouncementSeverity;
  send_dm: boolean;
  created_by: string;
  created_at_ms: number;
  starts_at_ms: number;
  ends_at_ms?: number | null;
  ended_at_ms?: number | null;
}
export const listAnnouncements = () => get<PlatformAnnouncement[]>('/admin/announcements');
export const createAnnouncement = (body: {
  title: string;
  body: string;
  severity: AnnouncementSeverity;
  /** Outage or security notice (D-192): cannot be dismissed and is always sent as a DM. */
  critical?: boolean;
  send_dm: boolean;
  ends_at_ms?: number | null;
  reason: string;
}) => send<PlatformAnnouncement>('/admin/announcements', 'POST', body);
export const endAnnouncement = (id: string) =>
  send<PlatformAnnouncement>(`/admin/announcements/${id}`, 'DELETE');
/** Edit one that is still showing (G-69): title, message and end time only; 409 once it has ended. */
export const editAnnouncement = (
  id: string,
  body: { title?: string; body?: string; ends_at_ms?: number; reason?: string },
) => send<PlatformAnnouncement>(`/admin/announcements/${id}`, 'PATCH', body);

// ---- staff alerts (ADM-ALR-001) ----

export interface StaffAlertRow {
  id: string;
  kind: string;
  detail: string;
  created_at_ms: number;
  acked_at_ms?: number | null;
  acked_by?: string | null;
}
export const listStaffAlerts = () => get<StaffAlertRow[]>('/admin/alerts');
export const ackStaffAlert = (id: string) => send<StaffAlertRow>(`/admin/alerts/${id}/ack`, 'POST');

export const revokeStaff = (accountId: string) => send<void>(`/admin/staff/${accountId}`, 'DELETE');

// ---- settings and features ----

export interface SettingView {
  key: string;
  group: string;
  kind: 'integer' | 'number' | 'boolean' | 'text';
  description: string;
  min: number;
  max: number;
  value: unknown;
  default: unknown;
  is_default: boolean;
  updated_at_ms?: number | null;
}
export const listSettings = () => get<SettingView[]>('/admin/settings');
export const patchSettings = (values: Record<string, unknown>, reason: string) =>
  send<SettingView[]>('/admin/settings', 'PATCH', { values, reason });

export interface FeatureView {
  id: string;
  name: string;
  description: string;
  section: string;
  phase: number;
  scopes: string[];
  enabled: boolean;
  default_enabled: boolean;
  schema: unknown;
  settings: unknown;
}
export const listFeatures = () => get<FeatureView[]>('/admin/features');
export const patchFeature = (
  id: string,
  body: {
    enabled?: boolean;
    reset_enabled?: boolean;
    settings?: Record<string, unknown>;
    reason?: string;
  },
) => send<FeatureView>(`/admin/features/${id}`, 'PATCH', body);

// ---- access log ----

export type AccessKind = 'admin_action' | 'read' | 'private_read' | 'denied';
export interface AccessLogEntry {
  seq: number;
  at_us: number;
  actor_id: string;
  kind: AccessKind;
  action: string;
  target_kind?: string | null;
  target_id?: string | null;
  reason: string;
  detail: string;
  ip?: string | null;
  prev_hash: string;
  hash: string;
}
export interface AccessLogPage {
  items: AccessLogEntry[];
  next_before?: number | null;
}
export interface ChainReport {
  ok: boolean;
  rows_checked: number;
  first_bad_seq?: number | null;
  head_hash?: string | null;
  problem?: string | null;
}
export const listAccessLog = (p: {
  kind?: string;
  action?: string;
  actor_id?: string;
  target_id?: string;
  before?: number | null;
  limit?: number;
}) => get<AccessLogPage>(`/admin/access-log${qs(p)}`);
export const verifyAccessLog = () => get<ChainReport>('/admin/access-log/verify');

export interface InviteDescendants {
  nodes: InviteNode[];
  /** True when the server's node cap cut the result short. */
  truncated: boolean;
}
export const getInviteDescendants = (accountId: string, depth: number) =>
  get<InviteDescendants>(`/admin/accounts/${accountId}/invite-descendants${qs({ depth })}`);

// ---- developer app review (REVIEW_APPS) ----

export interface DeveloperRequest {
  id: string;
  account_id: string;
  state: string;
  pitch: string;
  prior_work: string;
  created_at: string;
  reviewed_at?: string | null;
  reviewer_note?: string | null;
  account: { id: string; username?: string | null; global_name?: string | null; bot?: boolean };
}
export const listDeveloperRequests = (p: { state?: string; before?: string | null } = {}) =>
  get<DeveloperRequest[]>(`/admin/developers/requests${qs({ ...p, limit: 50 })}`);
export const decideDeveloperRequest = (id: string, approve: boolean, note: string) =>
  send<DeveloperRequest>(`/admin/developers/requests/${id}/decision`, 'POST', {
    approve,
    note: note || null,
  });

export interface AppVersion {
  id: string;
  application_id: string;
  version: number;
  state: string;
  manifest: unknown;
  needs_review: boolean;
  needs_review_reasons: string[];
  diff: { key: string; old?: unknown; new?: unknown }[];
  note?: string | null;
  reviewer_note?: string | null;
  created_at: string;
  decided_at?: string | null;
  application: { id: string; name: string; owner_id: string };
}
export type VersionDecision = 'approve' | 'reject' | 'request_changes';
export const listAppVersions = (p: { state?: string; before?: string | null } = {}) =>
  get<AppVersion[]>(`/admin/developers/versions${qs({ ...p, limit: 50 })}`);
export const decideAppVersion = (id: string, decision: VersionDecision, note: string) =>
  send<AppVersion>(`/admin/developers/versions/${id}/decision`, 'POST', {
    decision,
    note: note || null,
    approved_intents: [],
  });

// ---- cinema report queue (DELETE_CONTENT) ----

export interface CinemaReport {
  id: string;
  reporter_id: string;
  target_kind: 'review' | 'comment';
  target_id: string;
  reason: string;
  status: 'open' | 'actioned' | 'dismissed';
  created_at_ms: number;
  content?: {
    title_id: string;
    episode_id?: string | null;
    author_id: string;
    excerpt: string;
    hidden: boolean;
    created_at_ms: number;
  } | null;
}
export const listCinemaReports = (p: { status?: string; before?: string | null } = {}) =>
  get<{ items: CinemaReport[]; next_before?: string | null }>(
    `/admin/cinema/reports${qs({ ...p, limit: 24 })}`,
  );
export const resolveCinemaReport = (
  id: string,
  status: 'open' | 'actioned' | 'dismissed',
  reason: string,
) => send<void>(`/admin/cinema/reports/${id}/resolve`, 'POST', { status, reason: reason || null });
export const hideCinemaContent = (
  kind: 'review' | 'comment',
  id: string,
  hidden: boolean,
  reason: string,
) =>
  send<void>(`/admin/cinema/${kind === 'review' ? 'reviews' : 'comments'}/${id}/hidden`, 'PUT', {
    hidden,
    reason: reason || null,
  });

// ---- second factor window (ADM-STF-009) ----

export interface ReauthStatus {
  fresh: boolean;
  enrolled: boolean;
  window_secs: number;
  required: boolean;
}
/**
 * Not wrapped in withReauth: this is the call the dialog itself makes. Staff use the admin route; anyone else
 * (a server moderator under a 2FA requirement, `POST /guilds/{id}/mfa`) falls back to `/sr/auth/reauth`, which
 * opens the same per-session window.
 */
export const getReauthStatus = () =>
  apiFetch<ReauthStatus>('/admin/reauth', { retries: 0 }).catch((e: unknown) => {
    if (!(e instanceof ApiError) || e.status !== 403) throw e;
    return apiFetch<Omit<ReauthStatus, 'required'>>('/sr/auth/reauth', { retries: 0 }).then(
      (s) => ({
        ...s,
        required: true,
      }),
    );
  });
export const submitReauth = (method: 'totp' | 'recovery' | 'passkey', code: string) =>
  apiFetch<ReauthStatus>('/admin/reauth', {
    method: 'POST',
    retries: 0,
    json: { method, code },
  }).catch((e: unknown) => {
    if (!(e instanceof ApiError) || e.status !== 403) throw e;
    return apiFetch<Omit<ReauthStatus, 'required'>>('/sr/auth/reauth', {
      method: 'POST',
      retries: 0,
      json: { method, code },
    }).then((s) => ({ ...s, required: true }));
  });

// ---- private content viewer (ADM-COM-003) ----

export interface PrivateChannelRef {
  id: string;
  guild_id?: string | null;
  kind: number;
  name?: string | null;
  private: boolean;
  recipients: AccountRef[];
}
export interface PrivateEdit {
  edited_at_ms: number;
  previous_content: string;
}
export interface PrivateMessage {
  id: string;
  author_id: string;
  author_name: string;
  content: string;
  attachment_count: number;
  created_at_ms: number;
  edited_at_ms?: number | null;
  deleted_at_ms?: number | null;
  edits: PrivateEdit[];
  /** `visible`, `user_deleted` or `staff_removed`. */
  visibility: string;
  /** Set when the message was posted through a webhook; `author_*` is then its creator (D-235). */
  webhook?: { id: string; name: string } | null;
}
export interface PrivateMessagePage {
  channel: PrivateChannelRef;
  deleted_retention_days: number;
  items: PrivateMessage[];
  next_before?: string | null;
}
export const readPrivateMessages = (
  channelId: string,
  reason: string,
  before?: string | null,
  limit = 50,
) =>
  get<PrivateMessagePage>(
    `/admin/private/channels/${channelId}/messages${qs({ reason, before, limit })}`,
  );
export const listPrivateDms = (accountId: string, reason: string) =>
  get<PrivateChannelRef[]>(`/admin/private/accounts/${accountId}/dms${qs({ reason })}`);
export const listPrivateGuildChannels = (guildId: string, reason: string) =>
  get<PrivateChannelRef[]>(`/admin/private/guilds/${guildId}/channels${qs({ reason })}`);

// ---- Entrance copy (ENT-UI-010/025, ADM-PLT-003) ----

export type EntranceCopyKey = 'no-invitation' | 'house-notes' | 'landing-note' | 'hero-alt';
/** `GET /sr/entrance/copy` is public: the blocks staff overrode, `{}` when none. */
export const getEntranceCopy = () =>
  apiFetch<Partial<Record<EntranceCopyKey, string>>>('/sr/entrance/copy', { retries: 0 });
/** Needs a fresh second factor (`MANAGE_ENTRANCE`); a blank text removes the override. */
export const putEntranceCopy = (key: EntranceCopyKey, markdown: string, reason?: string) =>
  send<void>(`/admin/entrance/copy/${key}`, 'PUT', {
    markdown,
    ...(reason ? { reason } : {}),
  });
export const deleteEntranceCopy = (key: EntranceCopyKey) =>
  send<void>(`/admin/entrance/copy/${key}`, 'DELETE');

// ---- account drawer and Person additions (ADM-PPL-001/002/004, ADM-GRAPH-004) ----

export interface AdminSession {
  id: string;
  created_at_ms: number;
  last_seen_at_ms: number;
  expires_at_ms: number;
  ip?: string | null;
  user_agent?: string | null;
  device_label?: string | null;
}
/** Never carries token material; each read is written to the access log. */
export const listAccountSessions = (id: string) =>
  get<AdminSession[]>(`/admin/accounts/${id}/sessions`);
export interface AdminGuildRef {
  id: string;
  name: string;
  member_count: number;
  joined_at_ms: number;
  owner: boolean;
}
export const listAccountGuilds = (id: string) =>
  get<AdminGuildRef[]>(`/admin/accounts/${id}/guilds`);
export const freezeInvites = (id: string, reason: string) =>
  send<void>(`/admin/accounts/${id}/freeze-invites`, 'POST', reasonBody(reason));
export const unfreezeInvites = (id: string, reason: string) =>
  send<void>(`/admin/accounts/${id}/unfreeze-invites`, 'POST', reasonBody(reason));
export const setPersonNotes = (personId: string, notes: string, reason: string) =>
  send<void>(`/admin/persons/${personId}/notes`, 'PUT', { notes, ...(reason ? { reason } : {}) });
/** 409 while the Person is banned. */
export const linkAccount = (personId: string, account_id: string, reason: string) =>
  send<void>(`/admin/persons/${personId}/link`, 'POST', { account_id, reason });
/**
 * Move a Discord identity to another account (D-262). The target joins the Person when it is elsewhere.
 * 409 while the Person or either account is banned or suspended; needs a fresh second factor.
 */
export const moveIdentity = (
  personId: string,
  identityId: string,
  to_account_id: string,
  reason: string,
) =>
  send<{ to_account_id: string; linked_target: boolean; emailed: boolean }>(
    `/admin/persons/${personId}/identities/${identityId}/move`,
    'POST',
    { to_account_id, reason },
  );
/** 409 while banned or when it is the Person's only account; moves the account to a new Person. */
export const unlinkAccount = (id: string, reason: string) =>
  send<{ person_id: string }>(`/admin/accounts/${id}/unlink`, 'POST', reasonBody(reason));
export interface SuspicionEvent {
  id: string;
  created_at_ms: number;
  delta: number;
  level_after: number;
  reason: string;
  actor_id?: string | null;
  source_account_id?: string | null;
}
export const listSuspicionLog = (personId: string, limit = 50) =>
  get<SuspicionEvent[]>(`/admin/persons/${personId}/suspicion-log${qs({ limit })}`);

// ---- servers (ADM-COM-002) ----

export interface AdminGuild {
  id: string;
  name: string;
  owner_id: string;
  owner_username?: string | null;
  member_count: number;
  channel_count: number;
  created_at_ms: number;
  is_staff_guild: boolean;
  deleted: boolean;
  community_enabled: boolean;
}
export interface AdminGuildPage {
  items: AdminGuild[];
  next_after?: string | null;
}
export const listAdminGuilds = (p: { q?: string; after?: string | null; limit?: number } = {}) =>
  get<AdminGuildPage>(`/admin/guilds${qs(p)}`);
export const getAdminGuild = (id: string) => get<AdminGuild>(`/admin/guilds/${id}`);
export const transferGuild = (id: string, new_owner_id: string, reason: string) =>
  send<AdminGuild>(`/admin/guilds/${id}/transfer-ownership`, 'POST', { new_owner_id, reason });
export const deleteAdminGuild = (id: string, reason: string) =>
  send<void>(`/admin/guilds/${id}`, 'DELETE', reasonBody(reason));

// ---- Entrance hero image (H-41) ----

/** Needs a fresh second factor (`MANAGE_ENTRANCE`); `image` is a PNG, JPEG or WebP data URI (at most 8 MiB). */
export const putEntranceHero = (image: string, reason?: string) =>
  send<void>('/admin/entrance/hero', 'PUT', { image, ...(reason ? { reason } : {}) });
export const deleteEntranceHero = () => send<void>('/admin/entrance/hero', 'DELETE');

// ---- Server Directory queue (ADM-COM-001, H-40) ----

export type DirectoryStatus = 'pending' | 'approved' | 'rejected' | 'removed';
export interface DirectoryListing {
  guild_id: string;
  status: DirectoryStatus;
  category: string;
  description: string;
  featured: boolean;
  requested_at: string;
  decided_at: string | null;
  decision_note: string | null;
  guild_name: string | null;
  requested_by: string;
  approximate_member_count: number;
  /** Community is still on; a listing of a server that switched it off stays hidden. */
  community_enabled: boolean;
}
/** The listing's draft in the public directory card shape (`GET /admin/directory/queue/{id}/preview`). */
export interface DirectoryPreview {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  banner: string | null;
  category: string;
  featured: boolean;
  approximate_member_count: number;
  preferred_locale: string;
  vanity_url_code: string | null;
  /** The request is still waiting for a decision. */
  pending?: boolean;
}
export const listDirectoryQueue = (status: DirectoryStatus) =>
  get<DirectoryListing[]>(`/admin/directory${qs({ status })}`);
export const previewDirectoryListing = (guildId: string) =>
  get<DirectoryPreview>(`/admin/directory/queue/${guildId}/preview`);
export const decideDirectoryListing = (
  guildId: string,
  body: { decision: 'approve' | 'reject' | 'remove'; featured?: boolean; note?: string },
) => send<DirectoryListing>(`/admin/directory/${guildId}`, 'PATCH', body);

// ---- Activity review (REVIEW_APPS, DEV-ACT-001) ----

export interface PendingActivity {
  application_name: string;
  owner_id: string;
  config: {
    application_id: string;
    url_mappings: { prefix: string; target: string }[];
  };
  /** The bundle waiting for a decision, with the files it holds (J-12). */
  bundle: {
    bundle: {
      id: string;
      sha256: string;
      byte_size: number;
      entry_count: number;
    };
    entries: { path: string; size: number; content_type: string }[];
  };
}
export const listPendingActivities = () => get<PendingActivity[]>('/admin/developers/activities');
/** Where the reviewer downloads the zip of a submitted bundle. */
export const activityBundleUrl = (applicationId: string, bundleId: string) =>
  `/api/v1/admin/developers/activities/${applicationId}/bundles/${bundleId}/download`;
export const decideActivity = (
  applicationId: string,
  bundleId: string,
  approve: boolean,
  note: string,
) =>
  send<unknown>(`/admin/developers/activities/${applicationId}/decision`, 'POST', {
    decision: approve ? 'approve' : 'reject',
    bundle_id: bundleId,
    note: note || null,
  });

// ---- data-subject requests (M-25 step c, MOD-DSR) ----

export type DataRequestKind = 'export' | 'erasure';
export type DataRequestState = 'open' | 'verified' | 'running' | 'completed' | 'declined';
export interface DataRequest {
  id: string;
  kind: DataRequestKind;
  person_id: string;
  account_id: string;
  username: string;
  state: DataRequestState;
  note: string;
  received_at_ms: number;
  /** The date computed when the request was entered. */
  due_at_ms: number;
  extended_until_ms: number | null;
  extension_reason: string | null;
  /** The extended date when there is one, else `due_at_ms`. */
  effective_due_ms: number;
  overdue: boolean;
  verified_by: string | null;
  verified_at_ms: number | null;
  export_id: string | null;
  outcome: string | null;
  retained: string[];
  completed_at_ms: number | null;
  created_by: string;
}
export interface RetainedRecord {
  id: string;
  label: string;
  reason: string;
}
export interface DataRequestList {
  items: DataRequest[];
  deadline_days: number;
  retained_catalog: RetainedRecord[];
}
export const listDataRequests = (activeOnly: boolean) =>
  get<DataRequestList>(`/admin/data-requests${qs({ active_only: activeOnly, limit: 200 })}`);
export const createDataRequest = (body: {
  kind: DataRequestKind;
  account_id: string;
  received_at_ms?: number;
  note?: string;
}) => send<DataRequest>('/admin/data-requests', 'POST', body);
export const verifyDataRequest = (id: string, reason: string) =>
  send<DataRequest>(`/admin/data-requests/${id}/verify`, 'POST', { reason });
export const declineDataRequest = (id: string, reason: string) =>
  send<DataRequest>(`/admin/data-requests/${id}/decline`, 'POST', { reason });
export const extendDataRequest = (id: string, days: number, reason: string) =>
  send<DataRequest>(`/admin/data-requests/${id}/extend`, 'POST', { days, reason });
export const runDataRequest = (id: string) =>
  send<DataRequest>(`/admin/data-requests/${id}/run`, 'POST');
