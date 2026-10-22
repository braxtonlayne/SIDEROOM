// REST helpers for reports (members) and cases, holds and appeals (staff): /api/v1/moderation/*.
// Shapes mirror crates/sr-moderation (utoipa schemas). Ids are snowflake strings, times Unix milliseconds.
import { apiFetch } from '../api/client';

export type ReportCategory =
  | 'spam'
  | 'harassment'
  | 'illegal_content'
  | 'child_safety'
  | 'ban_evasion'
  | 'fraud'
  | 'platform_abuse'
  | 'nsfw_policy'
  | 'other';

/** The categories a member can pick, in the order of the dialog (spec section 8 plus "something else"). */
export const REPORT_CATEGORIES: ReportCategory[] = [
  'spam',
  'harassment',
  'nsfw_policy',
  'illegal_content',
  'child_safety',
  'fraud',
  'ban_evasion',
  'platform_abuse',
  'other',
];

/** The reasons the severity-only admin ban API (`BanReason`) knows; the rest go without one. */
export const BAN_API_REASONS: ReportCategory[] = [
  'illegal_content',
  'child_safety',
  'fraud',
  'platform_abuse',
  'harassment',
  'ban_evasion',
];

export type TargetKind =
  'message' | 'user' | 'channel' | 'guild' | 'library' | 'cinema' | 'person' | 'other';
export type CaseState = 'open' | 'triage' | 'investigating' | 'actioned' | 'dismissed' | 'closed';
export type Routing = 'guild' | 'platform' | 'reviewers';
export type Severity = 'standard' | 'serious';
export type ReportState = 'received' | 'under_review' | 'closed';

export interface NewReport {
  target_kind: TargetKind;
  target_id: string;
  channel_id?: string;
  guild_id?: string;
  category: ReportCategory;
  note: string;
  moderators_may_see_name: boolean;
}
export interface ReportReceipt {
  id: string;
  state: ReportState;
}
export interface MyReport {
  id: string;
  target_kind: TargetKind;
  target_id: string;
  category: ReportCategory;
  state: ReportState;
  outcome_line: string;
  created_at_ms: number;
}

export const fileReport = (body: NewReport) =>
  apiFetch<ReportReceipt>('/moderation/reports', { method: 'POST', retries: 0, json: body });
export const myReports = () => apiFetch<MyReport[]>('/moderation/reports/mine', { retries: 0 });

// ---- staff ----

export interface CaseSummary {
  id: string;
  state: CaseState;
  category: ReportCategory;
  severity: Severity;
  routing: Routing;
  escalated: boolean;
  summary: string;
  target_kind: TargetKind;
  target_id: string | null;
  /** The channel a message case is about. */
  target_channel_id?: string | null;
  guild_id: string | null;
  assignee_id: string | null;
  report_count: number;
  active_holds: number;
  opened_at_ms: number;
  updated_at_ms: number;
  closed_at_ms: number | null;
  /** The takedown notice this case was opened for (D-334). */
  takedown?: { notice_id: string; kind: TakedownKind };
}
export interface CasePage {
  items: CaseSummary[];
  next_cursor: string | null;
}
export interface CaseReport {
  id: string;
  category: ReportCategory;
  note: string;
  state: ReportState;
  moderators_may_see_name: boolean;
  reporter: { account_id: string; username: string } | null;
  created_at_ms: number;
}
export interface CaseEvidence {
  id: string;
  report_id: string | null;
  kind: string;
  payload: EvidencePayload;
  created_at_ms: number;
}
/** A message snapshot (`kind: message_snapshot`) or a target summary; the rest is free-form JSON. */
export interface EvidencePayload {
  messages?: {
    id: string;
    target: boolean;
    author_id: string;
    author_username: string | null;
    content: string;
    visibility: string;
    created_at_ms: number;
    attachments: {
      id: string;
      filename: string;
      content_type: string | null;
      size_bytes: number;
    }[];
  }[];
  label?: string;
  kind?: string;
  [k: string]: unknown;
}
export interface CaseNote {
  id: string;
  author_id: string;
  body: string;
  created_at_ms: number;
}
export interface CaseEvent {
  id: string;
  actor_id: string | null;
  kind: string;
  detail: Record<string, unknown>;
  created_at_ms: number;
}
export interface HoldView {
  id: string;
  case_id: string | null;
  scope_kind: string | null;
  scope_id: string | null;
  reason: string;
  created_by: string | null;
  created_at_ms: number;
  released_at_ms: number | null;
  release_reason: string | null;
  messages: number;
  attachments: number;
  submissions: number;
  /** Library and cinema rows (series, chapters, titles, episodes) the hold freezes. */
  catalog_rows?: number;
  review_due: boolean;
  /** A deletion request met this hold and staff have not decided yet (MOD-DSR-005). */
  erasure_flagged?: boolean;
  erasure_request_id?: string | null;
  /** The legal basis staff stated for keeping the held rows after the request. */
  erasure_retained_basis?: string | null;
}
export interface AppealView {
  id: string;
  ban_id: string;
  case_id: string | null;
  appellant_account_id: string;
  message: string;
  state: 'open' | 'upheld' | 'overturned';
  banned_by: string | null;
  person_id: string;
  decided_by: string | null;
  decision_reason: string;
  created_at_ms: number;
  decided_at_ms: number | null;
}
export interface CaseDetail {
  case: CaseSummary;
  subject_person_id: string | null;
  subject_account_id: string | null;
  linked_case_id: string | null;
  outcome: string;
  reports: CaseReport[];
  evidence: CaseEvidence[];
  notes: CaseNote[];
  events: CaseEvent[];
  holds: HoldView[];
  appeal: AppealView | null;
  access: 'read' | 'work' | 'manage';
}

const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });
const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });

export interface CaseQuery {
  state?: CaseState | '';
  open_only?: boolean;
  category?: ReportCategory | '';
  severity?: Severity | '';
  assignee?: string;
  guild_id?: string;
  subject_account_id?: string;
  cursor?: string | null;
  oldest_first?: boolean;
  /** `takedown`: only cases opened for a takedown notice. */
  source?: 'takedown' | '';
  limit?: number;
}
export const listCases = (p: CaseQuery = {}) => get<CasePage>(`/moderation/cases${qs({ ...p })}`);
export const getCase = (id: string) => get<CaseDetail>(`/moderation/cases/${id}`);
export const setCaseState = (
  id: string,
  body: { to: CaseState; outcome?: string; reporter_line?: string },
) => send<CaseSummary>(`/moderation/cases/${id}/state`, 'POST', body);
export const reopenCase = (id: string, reason: string) =>
  send<CaseSummary>(`/moderation/cases/${id}/reopen`, 'POST', { reason });
export const addCaseNote = (id: string, body: string) =>
  send<CaseNote>(`/moderation/cases/${id}/notes`, 'POST', { body });
export const claimCase = (id: string) =>
  send<CaseSummary>(`/moderation/cases/${id}/assignee`, 'PUT');
export const releaseCase = (id: string) =>
  send<CaseSummary>(`/moderation/cases/${id}/assignee`, 'DELETE');
export const escalateCase = (id: string, reason: string) =>
  send<CaseSummary>(`/moderation/cases/${id}/escalate`, 'POST', { reason });
export const recordCaseAction = (id: string, action: string, note: string) =>
  send<CaseSummary>(`/moderation/cases/${id}/actions`, 'POST', { action, note });
export type PerformedAction =
  | 'delete_message'
  | 'restore_message'
  | 'remove_attachment'
  | 'timeout'
  | 'kick'
  | 'guild_ban'
  | 'suspend_account';
/** Do the action through the server's services on the case's own target (N-21). */
export const performCaseAction = (
  id: string,
  action: PerformedAction,
  note: string,
  timeoutMinutes?: number,
  attachmentId?: string,
) =>
  send<CaseSummary>(`/moderation/cases/${id}/actions`, 'POST', {
    action,
    note,
    perform: true,
    ...(timeoutMinutes ? { timeout_minutes: timeoutMinutes } : {}),
    ...(attachmentId ? { attachment_id: attachmentId } : {}),
  });
export const warnCase = (id: string, text: string) =>
  send<{ delivered: boolean }>(`/moderation/cases/${id}/warn`, 'POST', { text });
export const banFromCase = (
  id: string,
  body: { reason_category: ReportCategory; severity?: Severity; note: string },
) => send<{ ban_id: string }>(`/moderation/cases/${id}/ban`, 'POST', body);

export type HoldKind = 'message' | 'attachment' | 'submission' | 'user_messages';
export const placeHold = (
  caseId: string,
  body: { kind: HoldKind; id: string; reason: string; since_ms?: number; until_ms?: number },
) =>
  send<{ hold_id: string; messages: number; attachments: number; submissions: number }>(
    `/moderation/cases/${caseId}/holds`,
    'POST',
    body,
  );
export const listHolds = (p: { active_only?: boolean; limit?: number } = {}) =>
  get<HoldView[]>(`/moderation/holds${qs(p)}`);
export const releaseHold = (id: string, reason: string) =>
  send<{ messages: number; attachments: number; submissions: number; purgeable_messages: number }>(
    `/moderation/holds/${id}/release`,
    'POST',
    { reason },
  );
export const markHoldReviewed = (id: string) =>
  send<void>(`/moderation/holds/${id}/reviewed`, 'POST');

export const listAppeals = (p: { open_only?: boolean; limit?: number } = {}) =>
  get<AppealView[]>(`/moderation/appeals${qs(p)}`);
export const decideAppeal = (id: string, decision: 'upheld' | 'overturned', reason: string) =>
  send<AppealView>(`/moderation/appeals/${id}/decision`, 'POST', { decision, reason });
/** The ban screen: the credentials prove the (session-less) banned account. */
export const fileAppeal = (body: { login: string; password: string; message: string }) =>
  send<void>('/moderation/appeals', 'POST', body);

export interface OpenCaseBody {
  category: ReportCategory;
  target_kind: TargetKind;
  target_id?: string;
  subject_account_id?: string;
  guild_id?: string;
  severity?: Severity;
  summary?: string;
}
/** Open a case without a report (`MANAGE_CASES`). */
export const openCase = (body: OpenCaseBody) =>
  send<CaseSummary>('/moderation/cases', 'POST', body);

// ---- the takedown register (step 2; MOD-TKD-*) ----

export type TakedownKind = 'copyright' | 'illegal_content';
export type NoticeState =
  'received' | 'triage' | 'rejected' | 'removed' | 'counter_noticed' | 'restored' | 'withdrawn';
export type TakedownItemKind =
  | 'message'
  | 'attachment'
  | 'library_series'
  | 'library_chapter'
  | 'cinema_title'
  | 'cinema_episode';
export type TakedownItemState = 'identified' | 'held' | 'disabled' | 'restored';

export interface NoticeSummary {
  id: string;
  kind: TakedownKind;
  state: NoticeState;
  notifier_name: string;
  received_at_ms: number;
  triage_due_at_ms: number;
  triage_overdue: boolean;
  case_id: string | null;
  items: number;
  removed_at_ms: number | null;
  restore_after_ms: number | null;
  court_filing_noted: boolean;
}
export interface TakedownItem {
  id: string;
  kind: TakedownItemKind;
  target_id: string;
  label: string;
  subject_account_id: string | null;
  state: TakedownItemState;
  hold_id: string | null;
  disabled_at_ms: number | null;
  restored_at_ms: number | null;
}
export interface NoticeEvent {
  id: string;
  actor_id: string | null;
  kind: string;
  detail: Record<string, unknown>;
  created_at_ms: number;
}
export interface CountedNotice {
  id: string;
  kind: TakedownKind;
  state: NoticeState;
  removed_at_ms: number;
  /** A counter-notice is pending. */
  contested: boolean;
}
export interface StrikeCount {
  person_id: string;
  strikes: number;
  contested: number;
  counted: CountedNotice[];
  window_months: number;
  threshold: number;
  review_case_id: string | null;
}
export interface NoticeDetail extends Omit<NoticeSummary, 'items'> {
  notifier_contact: string;
  legal_basis: string;
  locations: string[];
  original_text: string;
  entered_by: string;
  rejection_reason: string;
  counter_notice_text: string;
  counter_noticed_at_ms: number | null;
  restored_at_ms: number | null;
  restore_reason: string;
  items: TakedownItem[];
  events: NoticeEvent[];
  strikes: StrikeCount[];
}
export interface NewNotice {
  kind: TakedownKind;
  notifier_name: string;
  notifier_contact: string;
  legal_basis: string;
  locations: string[];
  original_text: string;
  received_at_ms?: number;
}

const tk = '/moderation/takedowns';
export const listNotices = (p: { state?: NoticeState | ''; limit?: number } = {}) =>
  get<NoticeSummary[]>(`${tk}${qs({ ...p })}`);
export const createNotice = (body: NewNotice) => send<NoticeDetail>(tk, 'POST', body);
export const getNotice = (id: string) => get<NoticeDetail>(`${tk}/${id}`);
export const addNoticeItem = (id: string, kind: TakedownItemKind, target: string) =>
  send<NoticeDetail>(`${tk}/${id}/items`, 'POST', { kind, id: target });
export const removeNoticeItem = (id: string, itemId: string) =>
  send<NoticeDetail>(`${tk}/${id}/items/${itemId}`, 'DELETE');
export const holdNotice = (id: string, reason: string) =>
  send<NoticeDetail>(`${tk}/${id}/hold`, 'POST', { reason });
export const rejectNotice = (id: string, reason: string) =>
  send<{ reply: string }>(`${tk}/${id}/reject`, 'POST', { reason });
export const upholdNotice = (id: string, reason: string) =>
  send<{ items_disabled: number; notified: number; review_case_id: string | null }>(
    `${tk}/${id}/uphold`,
    'POST',
    { reason },
  );
export const counterNotice = (id: string, text: string) =>
  send<NoticeDetail>(`${tk}/${id}/counter-notice`, 'POST', { text });
export const courtFiling = (id: string, noted: boolean, reason: string) =>
  send<NoticeDetail>(`${tk}/${id}/court-filing`, 'POST', { noted, reason });
export const restoreNotice = (id: string, reason: string) =>
  send<NoticeDetail>(`${tk}/${id}/restore`, 'POST', { reason });
export const withdrawNotice = (id: string, reason: string) =>
  send<NoticeDetail>(`${tk}/${id}/withdraw`, 'POST', { reason });
/** Where the register download lives (a plain navigation; the session cookie authorises it). */
export const registerExportUrl = (format: 'csv' | 'json') => `/api/v1${tk}/export?format=${format}`;

// ---- what the Library reader and the Cinema player show about a held item (MOD-TKD-011) ----

export type ItemHoldKind = 'library_series' | 'library_chapter' | 'cinema_title' | 'cinema_episode';
export type ItemHoldState = 'none' | 'held' | 'taken_down';
export const itemStatus = (kind: ItemHoldKind, id: string) =>
  get<{ state: ItemHoldState }>(`/moderation/item-status${qs({ kind, id })}`);

// ---- dashboards (MOD-DASH-002, -005 to -010; spec section 9) ----

export interface DashCount {
  key: string;
  count: number;
}
export interface DashInbox {
  groups: number;
  reports: number;
  oldest_seconds?: number | null;
}
export interface DashHealth {
  window_days: number;
  reports_per_day: { day: string; reports: number }[];
  reports: number;
  cases_opened: number;
  median_first_action_seconds?: number | null;
  cases_with_first_action: number;
  cases_waiting: number;
  closed: number;
  reopened: number;
  reopen_rate?: number | null;
  appeal_outcomes: DashCount[];
  appeals_open: number;
}
/** The sign-in history purge (D-347): no hold exception, so it shows whether the job keeps up. */
export interface DashSignIns {
  window_days: number;
  past_window: number;
  past_window_capped: boolean;
  last_purged?: number | null;
  last_run_at_ms?: number | null;
  backlog: boolean;
}
export interface DashRetention {
  rows: { key: string; count: number; capped: boolean }[];
  holds_active: number;
  holds_review_due: number;
  /** Holds a deletion request met, waiting for release or retain (MOD-DSR-005). */
  holds_erasure_pending: number;
  sign_ins: DashSignIns;
}
export interface DashAge {
  window_days: number;
  restricted_channels: number;
  restricted_series: number;
  restricted_titles: number;
  age_gate_resets: number;
  child_safety_open_cases: number;
  child_safety_reports: number;
  child_safety_active_bans: number;
}
export interface DashBans {
  active: number;
  by_reason: DashCount[];
  by_severity: DashCount[];
  by_age: DashCount[];
  lifted_in_window: number;
}
export interface DashTakedowns {
  window_days: number;
  undecided: number;
  triage_overdue: number;
  counter_noticed: number;
  restore_due: number;
  removed_in_window: number;
  received_in_window: number;
}
export interface DashDataRequests {
  window_days: number;
  open_exports: number;
  open_erasures: number;
  overdue: number;
  due_soon: number;
  completed_in_window: number;
}
/** Each section is absent when the viewer lacks the bit that reads it. */
export interface Dashboard {
  generated_at_ms: number;
  window_days: number;
  inbox?: DashInbox | null;
  health?: DashHealth | null;
  retention?: DashRetention | null;
  age?: DashAge | null;
  bans?: DashBans | null;
  takedowns?: DashTakedowns | null;
  data_requests?: DashDataRequests | null;
}
export const getDashboard = (days = 30) => get<Dashboard>(`/moderation/dashboard${qs({ days })}`);

export interface InboxGroup {
  case_id: string;
  summary: string;
  target_kind: TargetKind;
  severity: Severity;
  categories: ReportCategory[];
  reports: number;
  first_report_at_ms: number;
  last_report_at_ms: number;
}
export interface InboxPage {
  items: InboxGroup[];
  next_cursor?: string | null;
}
export interface InboxDismissOutcome {
  dismissed: string[];
  skipped: { case_id: string; reason: string }[];
}
export const listInbox = (p: { cursor?: string | null; limit?: number } = {}) =>
  get<InboxPage>(`/moderation/dashboard/inbox${qs(p)}`);
export const dismissInbox = (caseIds: string[], reason: string) =>
  send<InboxDismissOutcome>('/moderation/dashboard/inbox/dismiss', 'POST', {
    case_ids: caseIds,
    reason,
  });
// ---- inviter suspicion and evasion signals (MOD-SUS-001 to -004; spec section 10) ----

export interface AccountRef {
  id: string;
  username: string;
}
export interface SuspicionPanelEvent {
  id: string;
  delta: number;
  level_after: number;
  /** `raised`, or `cleared` (a clear keeps the events and carries the reason). */
  kind: 'raised' | 'cleared' | 'reregistration_refused';
  reason: string;
  source_account: AccountRef | null;
  actor_id: string | null;
  case_id: string | null;
  created_at_ms: number;
}
export interface BannedDescendant {
  account: AccountRef;
  person_id: string;
  depth: number;
  ban_category: 'serious' | 'standard';
  reason_category: ReportCategory | null;
  banned_at_ms: number;
  lifted: boolean;
  case_id: string | null;
}
export interface InvitePath {
  account_id: string;
  nodes: AccountRef[];
}
export interface SuspicionCaseRef {
  id: string;
  state: CaseState;
  auto_kind: 'suspicion_freeze' | 'suspicion_escalation';
  assignee_id: string | null;
  linked_case_id: string | null;
  opened_at_ms: number;
}
export type SignalKind = 'shared_login_ip' | 'shared_invite_lineage' | 'new_account_after_ban';
export type SignalState = 'open' | 'dismissed' | 'confirmed';
export interface EvasionSignal {
  id: string;
  person_id: string;
  account: AccountRef | null;
  related_person_id: string;
  related_account: AccountRef | null;
  kind: SignalKind;
  detail: Record<string, unknown>;
  state: SignalState;
  created_at_ms: number;
  reviewed_by: string | null;
  reviewed_at_ms: number | null;
  review_reason: string | null;
  case_id: string | null;
}
export interface SharedAddress {
  ip: string;
  person_last_seen_ms: number;
  related_last_seen_ms: number;
  persons: number;
}
export interface SignalDetail {
  signal: EvasionSignal;
  window_days: number;
  shared_addresses: SharedAddress[];
  /** What the reviewer looked at when confirming; stays after the sign-in rows are deleted. */
  snapshot: Record<string, unknown> | null;
}
export interface SuspicionPanel {
  person_id: string;
  level: number;
  freeze_threshold: number;
  escalate_threshold: number;
  invites_paused: boolean;
  accounts: AccountRef[];
  events: SuspicionPanelEvent[];
  banned_descendants: BannedDescendant[];
  invite_paths: InvitePath[];
  cases: SuspicionCaseRef[];
  /** Absent for staff without `MANAGE_ACCOUNTS`: the signals are IP-derived. */
  signals: EvasionSignal[] | null;
}
export const getSuspicionPanel = (personId: string) =>
  get<SuspicionPanel>(`/moderation/persons/${personId}/suspicion`);
export const countEvasionSignals = () => get<{ open: number }>('/moderation/evasion-signals/count');
export const linkSignalCase = (id: string, caseId: string) =>
  send<EvasionSignal>(`/moderation/evasion-signals/${id}/link-case`, 'POST', { case_id: caseId });
export const scanEvasion = (personId: string) =>
  send<{ raised: number }>(`/moderation/persons/${personId}/evasion-scan`, 'POST');
export const listEvasionSignals = (p: { state?: SignalState | 'all'; limit?: number } = {}) =>
  get<EvasionSignal[]>(`/moderation/evasion-signals${qs(p)}`);
export const getEvasionSignal = (id: string) =>
  get<SignalDetail>(`/moderation/evasion-signals/${id}`);
export const reviewEvasionSignal = (
  id: string,
  decision: 'dismissed' | 'confirmed',
  reason: string,
) => send<EvasionSignal>(`/moderation/evasion-signals/${id}/review`, 'POST', { decision, reason });
/** Keep the rows of a hold a deletion request met, with a stated legal basis (MOD-DSR-005). */
export const retainHoldAfterErasure = (id: string, basis: string) =>
  send<void>(`/moderation/holds/${id}/retain`, 'POST', { basis });

export type AgeTargetKind = 'channel' | 'library_series' | 'cinema_title';
export type AgeBand = 'under_18' | 'under_13';
export interface AgeChange {
  kind: AgeTargetKind;
  id: string;
  label: string;
  restricted: boolean;
  changed: boolean;
}
export interface UnderageOutcome {
  account_id: string;
  band: AgeBand;
  suspended: boolean;
  member_notified: boolean;
  inviter_notified: boolean;
  erasure_request_id?: string | null;
}
/** Mark or unmark a channel, series or title as age-restricted (MOD-AGE-001). */
export const setAgeRestricted = (body: {
  kind: AgeTargetKind;
  id: string;
  restricted: boolean;
  reason: string;
  case_id?: string;
}) => send<AgeChange>('/moderation/age/restrictions', 'POST', body);
/** Reset and lock an underage account's age confirmation; under 13 also suspends it (MOD-AGE-003). */
export const handleUnderage = (body: {
  account_id: string;
  band: AgeBand;
  note: string;
  case_id?: string;
}) => send<UnderageOutcome>('/moderation/age/underage', 'POST', body);
export const liftAgeLock = (accountId: string, reason: string) =>
  send<void>(`/moderation/age/underage/${accountId}/lift`, 'POST', { reason });
