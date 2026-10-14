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
  };
  if (opts.hsts) h['Strict-Transport-Security'] = HSTS;
  return h;
}
