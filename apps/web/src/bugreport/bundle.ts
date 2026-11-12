import { CAPS, ENGINES, idLikePiece, suspicious, type CapName } from '../crash/allowList';
import {
  GATEWAY_STATUSES,
  MAX_CRASH_FINGERPRINTS,
  MAX_FAILED_REQUESTS,
  SW_STATES,
  type BugBundle,
  type BugEngine,
  type BugGatewayStatus,
  type BugSwState,
  type BugTheme,
  type FailedRequest,
} from './contract';
import { templateOf } from './pathTemplate';

/**
 * Small readers the caller injects (so `buildBundle` is pure). Every one may return anything: the bundle is built from
 * named fields and keeps only what has the right shape, as `buildCrashCard` does. Nothing is ever spread.
 */
export interface BundleSources {
  commit: () => unknown;
  route: () => unknown;
  /** `{ w, h, dpr }` */
  viewport: () => unknown;
  /** The crash module's capability booleans. */
  caps: () => unknown;
  engine: () => unknown;
  locale: () => unknown;
  /** The applied theme id (`lounge-editorial`, `lounge-dark`, ...). */
  theme: () => unknown;
  online: () => unknown;
  /** `{ status, reconnects?, heartbeat_ms? }`: the store's connection status; the other two only if something keeps them. */
  gateway: () => unknown;
  sw: () => unknown;
  voiceConnected: () => unknown;
  /** `performance.getEntriesByType('resource')`. */
  resources: () => unknown;
  /** `performance.now()`, the clock `startTime` is on. */
  perfNow: () => unknown;
  origin: () => unknown;
  crashes: () => unknown;
}

const obj = (v: unknown): Record<string, unknown> | null =>
  typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;

const safe = <T>(f: () => T): T | undefined => {
  try {
    return f();
  } catch {
    return undefined;
  }
};

const int = (v: unknown, min: number, max: number): number | null =>
  typeof v === 'number' && Number.isFinite(v) ? Math.min(max, Math.max(min, Math.round(v))) : null;

export function themeOf(id: unknown): BugTheme {
  if (id === 'lounge-editorial' || id === 'editorial') return 'editorial';
  if (id === 'lounge-light') return 'lounge-light';
  if (id === 'lounge-dark') return 'lounge-dark';
  return 'other';
}

function cleanRoute(r: unknown): string {
  if (typeof r !== 'string' || !r || r.length > 64 || !/^[A-Za-z0-9._:-]+$/.test(r))
    return 'unknown';
  if (r.split(/[._:-]/).some(idLikePiece) || suspicious(r)) return 'unknown';
  return r;
}

function cleanLocale(l: unknown): string {
  if (typeof l !== 'string' || l.length > 35) return 'und';
  return /^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8}){0,3}$/.test(l) ? l : 'und';
}

function cleanGateway(g: unknown): BugBundle['gateway'] {
  const raw = obj(g);
  const s = raw?.status;
  // The store also knows `idle` (never opened): the contract has four words, and a client that is not connected is `closed`.
  const status: BugGatewayStatus = (GATEWAY_STATUSES as readonly unknown[]).includes(s)
    ? (s as BugGatewayStatus)
    : 'closed';
  const out: BugBundle['gateway'] = { status };
  const reconnects = int(raw?.reconnects, 0, 1_000_000);
  if (reconnects !== null) out.reconnects = reconnects;
  const hb = int(raw?.heartbeat_ms, 0, 600_000);
  if (hb !== null) out.heartbeat_ms = hb;
  return out;
}

/** Failed same-origin `/api/` requests from resource timing entries: newest 20. `method` is `?` (timing does not carry it). */
export function failedRequestsFrom(
  entries: unknown,
  origin: string,
  perfNow: number,
): FailedRequest[] {
  if (!Array.isArray(entries)) return [];
  const found: { r: FailedRequest; at: number }[] = [];
  for (const e of entries) {
    // A PerformanceResourceTiming is a class instance, not a plain object: read its named fields only.
    if (typeof e !== 'object' || e === null) continue;
    const o = e as Record<string, unknown>;
    const status = o.responseStatus;
    if (typeof status !== 'number' || !Number.isFinite(status) || status < 400 || status > 599)
      continue;
    if (typeof o.name !== 'string') continue;
    const path = templateOf(o.name, origin);
    if (!path) continue;
    const ms = int(o.duration, 0, 600_000) ?? 0;
    const start = typeof o.startTime === 'number' && Number.isFinite(o.startTime) ? o.startTime : 0;
    const ago = Math.max(0, Math.round((perfNow - (start + ms)) / 1000));
    found.push({ r: { method: '?', path, status: Math.round(status), ms, ago_s: ago }, at: start });
  }
  return found
    .sort((a, b) => b.at - a.at)
    .slice(0, MAX_FAILED_REQUESTS)
    .map((f) => f.r);
}

export function buildBundle(src: BundleSources, now: number = Date.now()): BugBundle {
  const commitRaw = safe(src.commit);
  const commit =
    typeof commitRaw === 'string' && /^[0-9a-f]{7,40}$/.test(commitRaw) ? commitRaw : '0000000';

  const vp = obj(safe(src.viewport));
  const dprRaw = vp?.dpr;
  const dpr =
    typeof dprRaw === 'number' && Number.isFinite(dprRaw)
      ? Math.min(8, Math.max(0.5, Math.round(dprRaw * 100) / 100))
      : 1;

  const capsRaw = obj(safe(src.caps));
  const engineRaw = safe(src.engine);
  const engine: BugEngine = (ENGINES as readonly unknown[]).includes(engineRaw)
    ? (engineRaw as BugEngine)
    : 'other';
  const caps: BugBundle['caps'] = { engine };
  for (const name of CAPS as readonly CapName[]) {
    if (typeof capsRaw?.[name] === 'boolean') caps[name] = capsRaw[name] as boolean;
  }

  const swRaw = safe(src.sw);
  const sw: BugSwState = (SW_STATES as readonly unknown[]).includes(swRaw)
    ? (swRaw as BugSwState)
    : 'none';

  const originRaw = safe(src.origin);
  const perfRaw = safe(src.perfNow);
  const crashesRaw = safe(src.crashes);
  const crashes = Array.isArray(crashesRaw)
    ? crashesRaw
        .filter((c): c is string => typeof c === 'string' && /^[0-9a-f]{64}$/.test(c))
        .slice(-MAX_CRASH_FINGERPRINTS)
    : [];

  return {
    commit,
    route: cleanRoute(safe(src.route)),
    at: int(now, 0, Number.MAX_SAFE_INTEGER) ?? 0,
    viewport: { w: int(vp?.w, 0, 16384) ?? 0, h: int(vp?.h, 0, 16384) ?? 0, dpr },
    caps,
    locale: cleanLocale(safe(src.locale)),
    theme: themeOf(safe(src.theme)),
    online: safe(src.online) === true,
    gateway: cleanGateway(safe(src.gateway)),
    sw,
    voice: { connected: safe(src.voiceConnected) === true },
    failed_requests: failedRequestsFrom(
      safe(src.resources),
      typeof originRaw === 'string' ? originRaw : '',
      typeof perfRaw === 'number' && Number.isFinite(perfRaw) ? perfRaw : 0,
    ),
    crashes,
  };
}
