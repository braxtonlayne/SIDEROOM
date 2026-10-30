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
 * - PRF-UNF-001 (app policy only; the public and no-script policies never frame anything): the privacy-enhanced
 *   embeds of YouTube (`youtube-nocookie.com`) and Vimeo (`player.vimeo.com`, `dnt=1`) are listed in
 *   `frame-src`; a PeerTube instance only when the operator names it (`SR_PEERTUBE_HOSTS`). The iframe is created
 *   only after the reader clicks, so the origin is allowed but nothing loads on render. `media-src` allows
 *   `https:` for the native player of a direct media link, also only after a click.
 */

/** The embed origins of the providers Play here supports. `crates/sr-api/src/security_headers.rs` pins them. */
export const EMBED_FRAME_ORIGINS = [
  'https://www.youtube-nocookie.com',
  'https://player.vimeo.com',
] as const;

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
    ['media-src', join(["'self'", 'blob:', 'https:'], opts.extraMedia)],
    ['font-src', "'self'"],
    ['connect-src', join(dev ? ["'self'", 'ws:', 'http:'] : ["'self'"], opts.extraConnect)],
    ['worker-src', "'self' blob:"],
    ['manifest-src', "'self'"],
    ['frame-src', join(["'self'", ...EMBED_FRAME_ORIGINS], opts.extraFrame)],
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
  /** `SR_PEERTUBE_HOSTS`: comma separated PeerTube instances whose embeds may be framed (PRF-UNF-001). */
  peertubeHosts?: string;
}

/** `a.example, https://B.example:443/` -> `['a.example', 'b.example']`; the same rules as `sr_unfurl::embed::parse_hosts`. */
export function parsePeertubeHosts(raw: string | undefined): string[] {
  const hosts: string[] = [];
  for (const part of (raw ?? '').split(',')) {
    const bare = part.trim().replace(/^https:\/\//, '').replace(/\/+$/, '');
    const host = (bare.split(':')[0] ?? '').toLowerCase();
    if (/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(host) && !hosts.includes(host)) hosts.push(host);
  }
  return hosts;
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
  for (const host of parsePeertubeHosts(cfg.peertubeHosts)) extraFrame.push(`https://${host}`);
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

export const SURFACES_BEGIN = '# surfaces:begin';
export const SURFACES_END = '# surfaces:end';

/** The slice of `public-surfaces.json` the block needs. */
export interface SurfaceTable {
  surfaces: {
    surface: string;
    kind: string;
    shell?: string;
    paths: string[];
    reserved?: boolean;
  }[];
  publicFiles: string[];
}

/** Public shells: scripts from this origin only; no wasm, no blob, no frames, no voice or activity origins. */
export const PUBLIC_CSP =
  "default-src 'self'; script-src 'self'; style-src 'self'; style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; manifest-src 'self'; worker-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; report-uri /internal/csp-report; report-to csp-endpoint";

/** The legal and error pages: plain HTML, no script, no fetch. */
export const NO_SCRIPT_CSP =
  "default-src 'none'; style-src 'self'; img-src 'self'; font-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; report-uri /internal/csp-report; report-to csp-endpoint";

const T = '\t';
const ROOT = '{$SR_WEB_ROOT:/srv/web}';
const UPSTREAM = '{$SR_UPSTREAM:127.0.0.1:8080}';
const REPORTING = 'header Reporting-Endpoints "csp-endpoint=\\"/internal/csp-report\\""';

/** The text between the markers (markers included), one tab deep, without a trailing newline. */
export function renderSurfacesBlock(table: SurfaceTable, publicCsp: string = PUBLIC_CSP): string {
  const lines: string[] = [`${T}${SURFACES_BEGIN}`];
  for (const s of table.surfaces) {
    if (s.kind !== 'bundle' || s.reserved || !s.shell) continue;
    lines.push(
      `${T}@${s.surface} path ${s.paths.join(' ')}`,
      `${T}handle @${s.surface} {`,
      `${T}${T}root * ${ROOT}`,
      `${T}${T}rewrite * /${s.shell}`,
      `${T}${T}header Content-Security-Policy "${publicCsp}"`,
      `${T}${T}${REPORTING}`,
      `${T}${T}header Cache-Control "no-cache"`,
      `${T}${T}file_server`,
      `${T}}`,
      '',
    );
  }
  const legal = table.surfaces.find((s) => s.surface === 'legal');
  lines.push(
    `${T}# The legal page is plain HTML rendered by sr-server (the operator's country and contact are filled in there).`,
    `${T}@legal path ${(legal?.paths ?? ['/house-notes']).filter((p) => p === '/house-notes').join(' ')}`,
    `${T}handle @legal {`,
    `${T}${T}reverse_proxy ${UPSTREAM}`,
    `${T}}`,
    '',
    `${T}@publicFiles path ${table.publicFiles.join(' ')}`,
    `${T}handle @publicFiles {`,
    `${T}${T}root * ${ROOT}`,
    `${T}${T}@immutable path /entrance/* /fonts/*`,
    `${T}${T}header @immutable Cache-Control "public, max-age=31536000, immutable"`,
    `${T}${T}@revalidate path /sw.js /manifest.webmanifest /offline.html /offline.css`,
    `${T}${T}header @revalidate Cache-Control "no-cache"`,
    `${T}${T}file_server`,
    `${T}}`,
    `${T}${SURFACES_END}`,
  );
  return lines.join('\n');
}

/** Replaces the marker block of a Caddyfile; throws when a marker is missing (the file is not ent-gate's). */
export function replaceSurfacesBlock(caddyfile: string, block: string): string {
  const begin = caddyfile.indexOf(`${T}${SURFACES_BEGIN}`);
  const endAt = caddyfile.indexOf(SURFACES_END);
  if (begin < 0 || endAt < begin)
    throw new Error(`Caddyfile has no "${SURFACES_BEGIN}" … "${SURFACES_END}" block`);
  return caddyfile.slice(0, begin) + block + caddyfile.slice(endAt + SURFACES_END.length);
}

/**
 * Rewrites the public-surface block of `Caddyfile.prod` (between the `# surfaces:begin` and `# surfaces:end`
 * markers) from `public-surfaces.json`. The block carries the matchers, one handle per bundle surface in table
 * order, the legal and public-file handles and the public policy on every shell; the app policy is not in the
 * Caddyfile any more (`sr-server` sends it from `dist/app/csp.txt`).
 */
export function renderCaddyfile(caddyfile: string, table: SurfaceTable): string {
  return replaceSurfacesBlock(caddyfile, renderSurfacesBlock(table));
}
