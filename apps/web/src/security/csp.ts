/**
 * Content-Security-Policy and companion headers for the single-page app document (XC-SEC-001).
 *
 * One source of truth: the Vite dev/preview servers import this, `csp.test.ts` asserts that
 * `deploy/compose/caddy/Caddyfile.prod` carries the byte-identical policy, and docs/09/12 refer here.
 *
 * Design notes:
 * - `script-src 'self' 'wasm-unsafe-eval'` is the policy in docs/02 section 8: the Rust/WASM bundles
 *   need compile permission, nothing else. No inline or eval scripts.
 * - React sets inline `style=""` attributes, so styles allow `'unsafe-inline'` for attributes only;
 *   style elements stay `'self'` (the build emits CSS files).
 * - `'self'` in `connect-src` covers `/api` and the `/gateway` WebSocket (CSP3 matches ws/wss to the
 *   same host). Voice signalling rides the gateway origin; media is WebRTC and is not governed by CSP.
 *   A separate voice or CDN origin goes through `extraConnect`.
 * - Activities are iframes on `*.activities.<host>` and are listed through `extraFrame`.
 */

/** Where browsers post violation reports (unauthenticated, rate limited, counters only; RDY-SEC-009). */
export const CSP_REPORT_PATH = '/internal/csp-report';
/** The Reporting API group the `report-to` directive names. */
export const CSP_REPORT_GROUP = 'csp-endpoint';
/** Value of the `Reporting-Endpoints` header that defines [CSP_REPORT_GROUP]. */
export const REPORTING_ENDPOINTS = `${CSP_REPORT_GROUP}="${CSP_REPORT_PATH}"`;

export interface CspOptions {
  /** Extra origins for `connect-src`, for example a separate voice signalling host. */
  extraConnect?: readonly string[];
  /** Extra origins for `frame-src`, for example `https://*.activities.example.com`. */
  extraFrame?: readonly string[];
  /** Extra origins for `img-src` / `media-src`, for example an external CDN. */
  extraMedia?: readonly string[];
  /** Relax for `vite dev`: HMR websocket and the react-refresh inline preamble. */
  dev?: boolean;
}

const join = (base: readonly string[], extra: readonly string[] = []) =>
  [...base, ...extra].join(' ');

export function buildCsp(opts: CspOptions = {}): string {
  const dev = opts.dev ?? false;
  const directives: [string, string][] = [
    ['default-src', "'self'"],
    ['script-src', dev ? "'self' 'wasm-unsafe-eval' 'unsafe-inline'" : "'self' 'wasm-unsafe-eval'"],
    ['style-src', "'self'"],
    ['style-src-attr', "'unsafe-inline'"],
    ['img-src', join(["'self'", 'data:', 'blob:'], opts.extraMedia)],
    ['media-src', join(["'self'", 'blob:'], opts.extraMedia)],
    ['font-src', "'self'"],
    ['connect-src', join(dev ? ["'self'", 'ws:', 'http:'] : ["'self'"], opts.extraConnect)],
    ['worker-src', "'self' blob:"],
    ['manifest-src', "'self'"],
    ['frame-src', join(["'self'"], opts.extraFrame)],
    ['object-src', "'none'"],
    ['base-uri', "'self'"],
    ['form-action', "'self'"],
    ['frame-ancestors', "'none'"],
    // RDY-SEC-009: violations are counted at POST /internal/csp-report (legacy `report-uri`, and `report-to` named in
    // the Reporting-Endpoints header).
    ['report-uri', CSP_REPORT_PATH],
    ['report-to', CSP_REPORT_GROUP],
  ];
  return directives.map(([k, v]) => `${k} ${v}`).join('; ');
}

/** `microphone`, `camera` and `display-capture` are the voice and video features; the rest is denied. */
export const PERMISSIONS_POLICY =
  'accelerometer=(), geolocation=(), gyroscope=(), magnetometer=(), payment=(), usb=(), interest-cohort=(), ' +
  'camera=(self), microphone=(self), display-capture=(self), fullscreen=(self), clipboard-write=(self)';

export const HSTS = 'max-age=31536000; includeSubDomains';

/** Headers for the app shell document. `hsts` only over https. */
export function securityHeaders(
  opts: CspOptions & { hsts?: boolean } = {},
): Record<string, string> {
  const h: Record<string, string> = {
    [opts.dev ? 'Content-Security-Policy-Report-Only' : 'Content-Security-Policy']: buildCsp(opts),
    'X-Content-Type-Options': 'nosniff',
    'Referrer-Policy': 'strict-origin-when-cross-origin',
    'Permissions-Policy': PERMISSIONS_POLICY,
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Reporting-Endpoints': REPORTING_ENDPOINTS,
  };
  if (opts.hsts) h['Strict-Transport-Security'] = HSTS;
  return h;
}

/** The deployment settings the policy depends on (H-27): mirrored from `config.toml`, never typed twice. */
export interface CspConfig {
  /** `server.public_url`, for example `https://chat.example.org`. */
  publicUrl?: string;
  /** `activities.enabled`: iframes on `*.activities.<host>` are allowed only then. */
  activitiesEnabled?: boolean;
  /** `voice.public_url`: listed in `connect-src` only when it names another origin than `publicUrl`. */
  voicePublicUrl?: string;
}

const originOf = (url: string | undefined): URL | null => {
  if (!url) return null;
  try {
    return new URL(url);
  } catch {
    return null;
  }
};

/** CSP options derived from the deployment settings. Unusable values add nothing (the policy stays tight). */
export function cspOptionsFromConfig(cfg: CspConfig): CspOptions {
  const site = originOf(cfg.publicUrl);
  const extraFrame: string[] = [];
  if (cfg.activitiesEnabled && site && site.protocol === 'https:')
    extraFrame.push(`https://*.activities.${site.host}`);
  const extraConnect: string[] = [];
  const voice = originOf(cfg.voicePublicUrl);
  if (voice && /^(https?|wss?):$/.test(voice.protocol)) {
    const secure = voice.protocol === 'https:' || voice.protocol === 'wss:';
    if (voice.host !== site?.host) {
      const http = `${secure ? 'https' : 'http'}://${voice.host}`;
      const ws = `${secure ? 'wss' : 'ws'}://${voice.host}`;
      extraConnect.push(http, ws);
    }
  }
  return { extraFrame, extraConnect };
}

/** `Content-Security-Policy` line of `Caddyfile.prod`; `deploy/scripts/render-caddyfile.mjs` rewrites it. */
export const caddyCspLine = (opts: CspOptions = {}) =>
  `header Content-Security-Policy "${buildCsp(opts)}"`;

/** Replaces the CSP line of a Caddyfile with the one built from `opts`. */
export function renderCaddyfile(caddyfile: string, opts: CspOptions = {}): string {
  return caddyfile.replace(/header Content-Security-Policy "[^"\n]*"/, caddyCspLine(opts));
}
