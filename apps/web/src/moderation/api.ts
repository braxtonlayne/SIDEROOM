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
  guild_id: string | null;
  assignee_id: string | null;
  report_count: number;
  active_holds: number;
  opened_at_ms: number;
  updated_at_ms: number;
  closed_at_ms: number | null;
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
  review_due: boolean;
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
