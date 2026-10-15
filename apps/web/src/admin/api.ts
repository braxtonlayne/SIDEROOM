// REST helpers for the Admin pages: /api/v1/admin/*. Shapes mirror crates/sr-admin (utoipa schemas).
// Ids are snowflake strings. Mutations never retry on 429 so the staff member sees the error.
import { apiFetch } from '../api/client';
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
export interface SystemView {
  counts: SystemCounts;
  gateway_sessions?: number | null;
  probes: Probe[];
  version: string;
}
export const getSystem = () => get<SystemView>('/admin/system');

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
export const banAccount = (id: string, category: BanCategory, reason: string) =>
  send<BanOutcome>(`/admin/accounts/${id}/ban`, 'POST', { category, reason });

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
export interface PersonDetail {
  id: string;
  accounts: AccountSummary[];
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
  send_dm: boolean;
  ends_at_ms?: number | null;
  reason: string;
}) => send<PlatformAnnouncement>('/admin/announcements', 'POST', body);
export const endAnnouncement = (id: string) =>
  send<PlatformAnnouncement>(`/admin/announcements/${id}`, 'DELETE');

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
/** Not wrapped in withReauth: this is the call the dialog itself makes. */
export const getReauthStatus = () => apiFetch<ReauthStatus>('/admin/reauth', { retries: 0 });
export const submitReauth = (method: 'totp' | 'recovery' | 'passkey', code: string) =>
  apiFetch<ReauthStatus>('/admin/reauth', { method: 'POST', retries: 0, json: { method, code } });

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
