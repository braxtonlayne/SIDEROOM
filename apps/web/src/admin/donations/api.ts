// REST helpers for Admin > Donations: /api/v1/admin/donations/* (docs/specs/donations.md section 6).
// Needs MANAGE_DONATIONS. Every write carries a reason that lands in the staff access log.
import { apiFetch } from '../../api/client';
import { withReauth } from '../reauth';
import type { InvoiceState, Invoice, Purpose, PerkSet, Rate } from '../../support/api';

export { errorText } from '../../settings/server/api';

// Setting the wallet is on the sensitive-action list (RDY-SEC-010): a closed second-factor window answers 900004.
const send = <T>(path: string, method: string, json?: unknown) =>
  withReauth(() =>
    apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) }),
  );
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });
const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

export interface RateSource {
  name: string;
  last_cents: number | null;
  last_ok_ms: number | null;
  last_error: string | null;
}
export interface Status {
  enabled: boolean;
  wallet: {
    network: string;
    fingerprint: string;
    next_index: number;
    gap_limit: number;
    gap_used: number;
  } | null;
  watcher: {
    last_run_ms: number | null;
    last_ok_ms: number | null;
    last_error: string | null;
    tip_height: number | null;
    polled: number;
  };
  counts: Record<string, number>;
  needs_attention: number;
  rate: Rate | null;
  rate_sources: RateSource[];
}
export const getStatus = () => get<Status>('/admin/donations/status');

export const testWallet = (descriptor: string, network: string) =>
  send<{ fingerprint: string; addresses: string[] }>('/admin/donations/wallet/test', 'POST', {
    descriptor,
    network,
  });
export const setWallet = (b: {
  descriptor: string;
  network: string;
  gap_limit?: number;
  reason: string;
}) =>
  send<{ network: string; fingerprint: string; addresses: string[] }>(
    '/admin/donations/wallet',
    'PUT',
    b,
  );

export interface AdminConfig {
  enabled: boolean;
  confirmations: number;
  finality_confirmations: number;
  invoice_expiry_minutes: number;
  rate_lock_minutes: number;
  tolerance_bps: number;
  late_watch_days: number;
  min_invoice_sats: number;
  min_donation_cents: number;
  max_donation_cents: number;
  max_open_invoices: number;
  poll_secs: number;
  patron_price_cents: number;
  patron_max_months: number;
  patron_days_per_month: number;
  grace_days: number;
  level_thresholds_cents: [number, number, number];
  pool_window_days: number;
  invite_credit_price_cents: number;
  max_invite_credits: number;
  hosting_cost_cents: number;
  free: PerkSet;
  patron: PerkSet;
  levels: [PerkSet, PerkSet, PerkSet];
  explorer_tx_url: string;
}
export const getConfig = () => get<AdminConfig>('/admin/donations/config');
export const patchConfig = (changes: Partial<AdminConfig>, reason: string) =>
  send<AdminConfig>('/admin/donations/config', 'PATCH', { changes, reason });

export interface AdminInvoice extends Invoice {
  user_id: string;
  user_name: string | null;
  derivation_index: number;
  resolution_note: string | null;
  resolved_by: string | null;
  audit: { at_ms: number; kind: string; detail: Record<string, unknown> }[];
}
export interface InvoiceFilters {
  state?: InvoiceState | '';
  purpose?: Purpose | '';
  q?: string;
  needs_attention?: boolean;
  before?: string | null;
  limit?: number;
}
export const listInvoices = (p: InvoiceFilters = {}) =>
  get<{ items: AdminInvoice[]; next_before: string | null }>(
    `/admin/donations/invoices${qs({ ...p })}`,
  );
export const getInvoice = (id: string) => get<AdminInvoice>(`/admin/donations/invoices/${id}`);
export const resolveInvoice = (
  id: string,
  b: { resolution: 'mark_paid' | 'refund_noted' | 'dismiss'; note: string; reason: string },
) => send<AdminInvoice>(`/admin/donations/invoices/${id}/resolve`, 'POST', b);

export interface Grant {
  id: string;
  kind: 'patron' | 'server_pool';
  user_id: string | null;
  guild_id: string | null;
  source: 'invoice' | 'admin' | 'migrated';
  source_id: string | null;
  days: number | null;
  ends_at_ms: number | null;
  permanent: boolean;
  usd_cents: number | null;
  starts_at_ms: number;
  revoked_at_ms: number | null;
  reason: string | null;
}
export const listGrants = (
  p: {
    kind?: string;
    user_id?: string;
    guild_id?: string;
    active?: boolean;
    before?: string | null;
  } = {},
) =>
  get<{ items: Grant[]; next_before: string | null }>(
    `/admin/donations/entitlements${qs({ ...p, limit: 25 })}`,
  );
export const grant = (b: {
  kind: 'patron' | 'server_pool';
  user_id?: string;
  guild_id?: string;
  days?: number;
  permanent?: boolean;
  usd_cents?: number;
  reason: string;
}) => send<Grant>('/admin/donations/entitlements', 'POST', b);
export const revokeGrant = (id: string, reason: string) =>
  send<Grant>(`/admin/donations/entitlements/${id}`, 'DELETE', { reason });

export interface Report {
  from_ms: number;
  to_ms: number;
  credited_cents: number;
  by_purpose: { purpose: string; cents: number; count: number }[];
  by_day: { day: string; cents: number }[];
  by_state: Record<string, number>;
}
export const getReport = (p: { from_ms?: number; to_ms?: number } = {}) =>
  get<Report>(`/admin/donations/report${qs(p)}`);

export const hideFromBoards = (b: {
  user_id?: string;
  guild_id?: string;
  hidden: boolean;
  reason: string;
}) => send<void>('/admin/donations/leaderboard/hide', 'POST', b);
