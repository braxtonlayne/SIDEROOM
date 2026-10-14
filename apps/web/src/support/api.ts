// REST calls for donations: /api/v1/donations/*, /guilds/{id}/support (docs/specs/donations.md section 6).
// Shapes mirror crates/sr-api/src/routes/donations/dto.rs. Ids are snowflake strings, money is integer USD
// cents and satoshis, times are Unix milliseconds. Mutations never retry on 429.
import { apiFetch } from '../api/client';

export { errorText } from '../settings/server/api';

const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });

const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

export type Purpose = 'donate' | 'patron' | 'server_level' | 'invite_credit';
export type InvoiceState =
  | 'open'
  | 'seen'
  | 'confirming'
  | 'confirmed'
  | 'overpaid'
  | 'late_paid'
  | 'underpaid'
  | 'expired'
  | 'cancelled'
  | 'resolved';

export interface VideoProfile {
  height: number;
  fps: number;
}
export interface PerkSet {
  upload_mib: number;
  video_profiles: VideoProfile[];
  video_senders: number;
  stream_viewers: number;
  audio_kbps: number;
  watch_parties: number;
  downloads_per_day: number;
  exempt_from_sender_limit: boolean;
}
export interface Limits {
  upload_bytes: number;
  large_upload_expiry_days: number | null;
  standard_upload_bytes: number;
  video_profiles: VideoProfile[];
  video_senders: number;
  stream_viewers: number;
  audio_kbps: number;
  watch_parties: number;
  downloads_per_day: number;
  exempt_from_sender_limit: boolean;
  patron: boolean;
  guild_level: number;
}

export interface Rate {
  cents_per_btc: number;
  source: string;
  age_secs: number;
  stale: boolean;
}
export interface DonationConfig {
  enabled: boolean;
  network: string | null;
  confirmations: number;
  invoice_expiry_minutes: number;
  rate_lock_minutes: number;
  patron: {
    price_cents: number;
    max_months: number;
    days_per_month: number;
    grace_days: number;
    perks: PerkSet;
  };
  levels: { level: number; threshold_cents: number; perks: PerkSet }[];
  free_perks: PerkSet;
  invite_credit_price_cents: number;
  max_invite_credits: number;
  min_donation_cents: number;
  max_donation_cents: number;
  rate: Rate | null;
  hosting: { cost_cents: number; donated_this_month_cents: number; month_start_ms: number };
}
export const getConfig = () => get<DonationConfig>('/donations/config');

export interface Tx {
  txid: string;
  amount_sats: number;
  confirmations: number;
  dropped: boolean;
}
export interface Invoice {
  id: string;
  purpose: Purpose;
  state: InvoiceState;
  usd_cents: number;
  sats: number;
  btc: string;
  address: string;
  bip21: string;
  network: string;
  guild_id: string | null;
  months: number | null;
  credits: number | null;
  anonymous: boolean;
  rate: Rate;
  confirmations_required: number;
  confirmations: number | null;
  received_sats: number;
  confirmed_sats: number;
  shortfall_sats: number;
  excess_sats: number;
  late: boolean;
  created_at_ms: number;
  expires_at_ms: number;
  quote_expires_at_ms: number;
  credited_at_ms: number | null;
  explorer_url: string | null;
  txs: Tx[];
  resolution: string | null;
}
export interface CreateInvoice {
  purpose: Purpose;
  usd_cents?: number;
  months?: number;
  guild_id?: string;
  credits?: number;
  anonymous?: boolean;
}
export const createInvoice = (b: CreateInvoice) => send<Invoice>('/donations/invoices', 'POST', b);
export const getInvoice = (id: string) => get<Invoice>(`/donations/invoices/${id}`);
export const checkInvoice = (id: string) =>
  send<Invoice>(`/donations/invoices/${id}/check`, 'POST');
export const cancelInvoice = (id: string) =>
  send<Invoice>(`/donations/invoices/${id}/cancel`, 'POST');
export const listInvoices = (p: { before?: string | null; limit?: number } = {}) =>
  get<{ items: Invoice[]; next_before: string | null }>(`/donations/invoices${qs(p)}`);

export interface Patron {
  active: boolean;
  in_grace: boolean;
  permanent: boolean;
  until_ms: number | null;
  started_ms: number | null;
}
export interface Prefs {
  hide_name: boolean;
  hide_amount: boolean;
  hide_badge: boolean;
}
export interface Me {
  patron: Patron;
  limits: Limits;
  preferences: Prefs;
}
export const getMe = () => get<Me>('/donations/me');
export const patchPrefs = (p: Partial<Prefs>) =>
  send<Prefs>('/donations/me/preferences', 'PATCH', p);

export interface Contributor {
  user_id: string | null;
  name: string;
  avatar: string | null;
  cents: number | null;
  anonymous: boolean;
}
export interface GuildSupport {
  guild_id: string;
  level: number;
  pool_cents: number;
  pool_window_days: number;
  next_threshold_cents: number | null;
  grace_until_ms: number | null;
  perks: PerkSet;
  top_contributors: Contributor[];
  hidden_from_leaderboard: boolean;
  is_owner: boolean;
}
export const getGuildSupport = (guildId: string) => get<GuildSupport>(`/guilds/${guildId}/support`);
export const patchGuildSupport = (guildId: string, hidden: boolean) =>
  send<{ hidden_from_leaderboard: boolean }>(`/guilds/${guildId}/support`, 'PATCH', {
    hidden_from_leaderboard: hidden,
  });

export interface BoardItem {
  rank: number;
  user_id: string | null;
  guild_id: string | null;
  name: string;
  avatar: string | null;
  cents: number | null;
  anonymous: boolean;
}
export interface Board {
  board: string;
  window: string;
  items: BoardItem[];
  me: { rank: number; cents: number } | null;
  generated_at_ms: number;
}
export const getBoard = (board: 'donors' | 'servers', window: '30d' | 'lifetime') =>
  get<Board>(`/donations/leaderboards/${board}${qs({ window })}`);

// ---- formatting ----

export const usd = (cents: number) =>
  new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(cents / 100);
export const sats = (n: number) => `${new Intl.NumberFormat('en-US').format(n)} sats`;
export const STATE_COPY: Record<InvoiceState, string> = {
  open: 'Waiting for payment',
  seen: 'Payment seen, waiting for a block',
  confirming: 'Confirming',
  confirmed: 'Paid',
  overpaid: 'Paid (extra received)',
  late_paid: 'Paid after expiry',
  underpaid: 'Underpaid',
  expired: 'Expired',
  cancelled: 'Cancelled',
  resolved: 'Closed by staff',
};
/** States the chain can still change. */
export const isLive = (s: InvoiceState) =>
  s === 'open' || s === 'seen' || s === 'confirming' || s === 'underpaid';
export const isPaid = (s: InvoiceState) =>
  s === 'confirmed' || s === 'overpaid' || s === 'late_paid';
export const PURPOSE_COPY: Record<Purpose, string> = {
  donate: 'Donation',
  patron: 'Patron',
  server_level: 'Server support',
  invite_credit: 'Invite credits',
};
