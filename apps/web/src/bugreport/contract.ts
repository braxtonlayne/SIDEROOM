/**
 * The wire contract of the beta bug reporter (SEC-AUD-165, D-647..D-650; `p64-plan.md` "The wire contract").
 * Hand-written for now; part 2 swaps these for the generated client's types where the report branch is merged.
 */
import type { CapName } from '../crash/allowList';

export const MAX_DESCRIPTION = 2000;
export const MAX_FAILED_REQUESTS = 20;
export const MAX_CRASH_FINGERPRINTS = 5;
export const MAX_PICTURE_BYTES = 2 * 1024 * 1024;
export const MAX_PICTURE_SIDE = 1600;
/** The ladder's last resort: a picture still too large at quality 0.7 is re-encoded at this long side. */
export const FALLBACK_PICTURE_SIDE = 1280;
export const MAX_PATH = 120;

export const THEMES = ['editorial', 'lounge-light', 'lounge-dark', 'other'] as const;
export type BugTheme = (typeof THEMES)[number];

export const GATEWAY_STATUSES = ['connected', 'connecting', 'reconnecting', 'closed'] as const;
export type BugGatewayStatus = (typeof GATEWAY_STATUSES)[number];

export const SW_STATES = ['none', 'installing', 'waiting', 'active'] as const;
export type BugSwState = (typeof SW_STATES)[number];

export const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', '?'] as const;
export type BugMethod = (typeof METHODS)[number];

/** The browser engine word the crash allow-list uses (the plan's example said `blink`; the crash list says `chromium`). */
export type BugEngine = 'chromium' | 'firefox' | 'webkit' | 'other';

export interface FailedRequest {
  method: BugMethod;
  path: string;
  status: number;
  ms: number;
  ago_s: number;
}

export interface BugBundle {
  commit: string;
  route: string;
  at: number;
  viewport: { w: number; h: number; dpr: number };
  caps: Partial<Record<CapName, boolean>> & { engine: BugEngine };
  locale: string;
  theme: BugTheme;
  online: boolean;
  /** `reconnects` and `heartbeat_ms` are left out when the client does not keep them. */
  gateway: { status: BugGatewayStatus; reconnects?: number; heartbeat_ms?: number };
  sw: BugSwState;
  voice: { connected: boolean };
  failed_requests: FailedRequest[];
  crashes: string[];
}

/** What the server writes for a `gateway` the body did not carry (SEC-AUD-266); a sent report never has it. */
export type StoredGatewayStatus = BugGatewayStatus | 'unknown';

/**
 * A report as staff read it back. The server (since SEC-AUD-266) always writes every key, with a neutral value for a
 * part the body lacked, but rows stored before that can be missing `viewport`, `gateway`, `voice`, `online` and
 * `sw`, and a row is read defensively whatever its age, so every key but the three the server never leaves out
 * (`commit`, `route`, `at`) is optional here and the staff page guards each read.
 */
export interface StoredBundle {
  commit: string;
  route: string;
  at: number;
  viewport?: { w: number; h: number; dpr: number };
  caps?: Partial<Record<CapName, boolean>> & { engine?: BugEngine };
  locale?: string;
  theme?: BugTheme;
  online?: boolean;
  gateway?: { status: StoredGatewayStatus; reconnects?: number; heartbeat_ms?: number };
  sw?: BugSwState;
  voice?: { connected: boolean };
  failed_requests?: FailedRequest[];
  crashes?: string[];
  masked?: number;
}

export interface BugScreenshot {
  type: 'image/jpeg';
  data: string;
  masked: number;
}

export interface BugReportBody {
  description: string;
  bundle: BugBundle;
  screenshot?: BugScreenshot;
}

export interface BugReportCreated {
  id: string;
}

export type BugStatus = 'open' | 'closed';

export interface BugReporter {
  id: string;
  username: string;
}

export interface BugReportRow {
  id: string;
  created_ms: number;
  status: BugStatus;
  route: string;
  commit: string;
  reporter: BugReporter;
  has_screenshot: boolean;
}

export interface BugReportList {
  reports: BugReportRow[];
  open_count: number;
  /** SEC-AUD-267: more reports exist after the last one: ask again with `before` set to its id. Absent from older servers. */
  has_more?: boolean;
  /** D-647: the beta reporter is on and the platform has 100 or more active accounts (no number is given). */
  beta_over_100?: boolean;
}

export interface BugReportDetail extends BugReportRow {
  description: string;
  bundle: StoredBundle;
  closed_ms: number | null;
  closed_by: BugReporter | null;
}
