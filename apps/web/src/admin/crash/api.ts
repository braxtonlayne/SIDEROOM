// REST helpers for Admin > Crash reports (OPS-CRASH-002): GET /admin/crash-reports and /admin/crash-reports/{id}
// (crates/sr-api/src/routes/crash). The server's authorization is the boundary; the page only decides what to show.
import { apiFetch } from '../../api/client';
import { withReauth } from '../reauth';

/** One group of crashes with the same fingerprint (the list never carries the card or the replay). */
export interface CrashSummary {
  id: string;
  fingerprint: string;
  commit: string;
  kind: string;
  route: string;
  count: number;
  first_seen_ms: number;
  last_seen_ms: number;
  /** Not sent today (the card is only in the detail); shown in the list when the server starts sending it. */
  top_frames?: string[];
}

export interface CrashFrame {
  file: string;
  line: number;
  col: number;
  fn?: string;
}

/** The allow-listed card. Every field is optional on purpose: the page shows what is there. */
export interface CrashCardView {
  frames?: CrashFrame[];
  caps?: Record<string, boolean>;
  engine?: string;
  at?: number;
}

export interface CrashTickView {
  t: number;
  src: string;
  input: string;
}

export interface CrashDetail extends CrashSummary {
  card: CrashCardView;
  replay: CrashTickView[];
}

/** The most the API returns in one list. */
export const LIST_LIMIT = 200;

export const listCrashReports = () =>
  withReauth(() =>
    apiFetch<CrashSummary[]>(`/admin/crash-reports?limit=${LIST_LIMIT}`, { retries: 0 }),
  );

/** Opening a report is the audited read (`crash.report.read`): call this only when a person asks to see one. */
export const getCrashReport = (id: string) =>
  withReauth(() =>
    apiFetch<CrashDetail>(`/admin/crash-reports/${encodeURIComponent(id)}`, { retries: 0 }),
  );
