// REST helpers for the graduated moderation toolset (MOD-GRD-001, D-395): per-member acts, subtree cases, the
// two-person requests, the Operator's branch action, a member's own restrictions and the transparency counts.
// Shapes mirror crates/sr-moderation/src/graduated_model.rs. Ids are snowflake strings, times Unix milliseconds.
// Staff calls go through `withReauth`: sterilization and branch-action requests, approvals and the branch action
// answer 403 900004 until the staff member has proved a second factor, then retry once.
import { apiFetch } from '../api/client';
import { withReauth } from '../admin/reauth';

export type RestrictionKind =
  'warn' | 'timeout' | 'invite_freeze' | 'case_freeze' | 'sterilization';

export interface RestrictionView {
  id: string;
  account_id: string;
  kind: RestrictionKind;
  reason: string;
  case_id: string | null;
  imposed_by: string;
  approved_by: string | null;
  weight: number;
  starts_at_ms: number;
  ends_at_ms: number;
  removes_account: boolean;
  state: 'active' | 'lifted' | 'overturned' | 'expired' | 'removed';
  appeal_message: string | null;
  appeal_state: 'open' | 'upheld' | 'overturned' | null;
}

export interface MyRestriction {
  id: string;
  kind: RestrictionKind;
  effect: string;
  reason: string;
  ends_at_ms: number;
  state: string;
  can_appeal: boolean;
  appeal_state: 'open' | 'upheld' | 'overturned' | null;
}

export type NodeStatus =
  | 'auto_frozen'
  | 'review'
  | 'frozen'
  | 'banned'
  | 'sterilized'
  | 'cleared'
  | 'removed'
  | 'not_active';

export interface SubtreeCaseSummary {
  id: string;
  root_account_id: string;
  root_username: string;
  category: string;
  state: 'open' | 'closed';
  opened_at_ms: number;
  nodes: number;
  auto_frozen: number;
  waiting_review: number;
}

export interface SubtreeNode {
  account_id: string;
  username: string;
  role: 'root' | 'descendant' | 'inviter' | 'inviter2';
  depth: number;
  parent_account_id: string | null;
  registered_at_ms: number;
  messages: number;
  voice_seconds: number;
  unactivated: boolean;
  status: NodeStatus;
  restricted: boolean;
  note: string;
}

export interface ModRequest {
  id: string;
  kind: 'sterilize' | 'branch_action';
  case_id: string;
  target_account_id: string | null;
  action: 'freeze' | 'ban' | null;
  reason: string;
  proposed_by: string;
  approved_by: string | null;
  state: 'pending' | 'approved' | 'executed' | 'rejected' | 'cancelled';
  created_at_ms: number;
  nodes_actioned: number | null;
}

export interface SubtreeCaseDetail {
  case: SubtreeCaseSummary;
  opened_by: string;
  nodes_truncated: boolean;
  nodes: SubtreeNode[];
  requests: ModRequest[];
}

export interface NoticeView {
  kind: 'serious_ban' | 'branch_action';
  category: string;
  title: string;
  body: string;
  accounts_actioned: number;
  accounts_frozen: number;
  created_at_ms: number;
}

export interface BranchOutcome {
  request_id: string;
  case_id: string;
  action: 'freeze' | 'ban';
  nodes_actioned: number;
  already_banned_or_gone: number;
  notice: NoticeView;
}

export interface TransparencyMonth {
  month: string;
  serious_bans: number;
  branch_actions: number;
  accounts_actioned: number;
  accounts_frozen: number;
  sterilizations: number;
  appeals_filed: number;
  appeals_overturned: number;
  by_category: { category: string; notices: number }[];
}

const base = '/admin/moderation';
const post = <T>(path: string, json?: unknown) =>
  withReauth(() => apiFetch<T>(path, { method: 'POST', retries: 0, json: json ?? {} }));
const get = <T>(path: string) => withReauth(() => apiFetch<T>(path, { retries: 0 }));

// ---- per-member acts ----
export const warnMember = (account: string, reason: string) =>
  post<RestrictionView>(`${base}/members/${account}/warn`, { reason });
export const timeoutMember = (account: string, minutes: number, reason: string) =>
  post<RestrictionView>(`${base}/members/${account}/timeout`, { minutes, reason });
export const freezeInvites = (account: string, days: number, reason: string) =>
  post<RestrictionView>(`${base}/members/${account}/invite-freeze`, { days, reason });
export const revokeInvites = (account: string, reason: string) =>
  post<{ codes_revoked: number }>(`${base}/members/${account}/revoke-invites`, { reason });
export const banMember = (account: string, reason_category: string, reason: string) =>
  post<unknown>(`${base}/members/${account}/ban`, { reason_category, reason });
export const liftRestriction = (id: string, reason: string) =>
  post<RestrictionView>(`${base}/restrictions/${id}/lift`, { reason });
export const memberActions = (account: string) =>
  get<RestrictionView[]>(`${base}/members/${account}/actions`);
export const restrictionAppeals = () => get<RestrictionView[]>(`${base}/restriction-appeals`);
export const decideRestrictionAppeal = (
  id: string,
  decision: 'upheld' | 'overturned',
  reason: string,
) => post<RestrictionView>(`${base}/restrictions/${id}/appeal-decision`, { decision, reason });

// ---- subtree cases ----
export const listCases = (open: boolean) =>
  get<SubtreeCaseSummary[]>(`${base}/subtree-cases${open ? '?open=true' : ''}`);
export const getCase = (id: string) => get<SubtreeCaseDetail>(`${base}/subtree-cases/${id}`);
export const clearNode = (caseId: string, account: string, reason: string) =>
  post<unknown>(`${base}/subtree-cases/${caseId}/nodes/${account}/clear`, { reason });
export const freezeNode = (caseId: string, account: string, reason: string) =>
  post<unknown>(`${base}/subtree-cases/${caseId}/nodes/${account}/freeze`, { reason });
export const banNode = (caseId: string, account: string, reason_category: string, reason: string) =>
  post<unknown>(`${base}/subtree-cases/${caseId}/nodes/${account}/ban`, {
    reason_category,
    reason,
  });
export const closeCase = (caseId: string, reason: string) =>
  post<unknown>(`${base}/subtree-cases/${caseId}/close`, { reason });
export const requestSterilization = (caseId: string, account: string, reason: string) =>
  post<ModRequest>(`${base}/subtree-cases/${caseId}/requests`, {
    kind: 'sterilize',
    account_id: account,
    reason,
  });
export const requestBranchAction = (caseId: string, action: 'freeze' | 'ban', reason: string) =>
  post<ModRequest>(`${base}/subtree-cases/${caseId}/requests`, {
    kind: 'branch_action',
    action,
    reason,
  });
export const listRequests = () => get<ModRequest[]>(`${base}/requests`);
export const approveRequest = (id: string) => post<ModRequest>(`${base}/requests/${id}/approve`);
export const rejectRequest = (id: string, reason: string) =>
  post<ModRequest>(`${base}/requests/${id}/reject`, { reason });
export const branchAction = (request_id: string) =>
  post<BranchOutcome>(`${base}/branch-action`, { request_id });

// ---- the member's side ----
export const myRestrictions = () =>
  apiFetch<MyRestriction[]>('/sr/moderation/restrictions', { retries: 0 });
export const appealRestriction = (id: string, message: string) =>
  apiFetch<MyRestriction>(`/sr/moderation/restrictions/${id}/appeal`, {
    method: 'POST',
    retries: 0,
    json: { message },
  });
export const transparency = () =>
  apiFetch<{ months: TransparencyMonth[] }>('/moderation/transparency', { retries: 0 });
