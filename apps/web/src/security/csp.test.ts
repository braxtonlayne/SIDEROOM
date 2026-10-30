import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import {
  NO_SCRIPT_CSP,
  PUBLIC_CSP,
  renderSurfacesBlock,
  buildCsp,
  cspOptionsFromConfig,
  PERMISSIONS_POLICY,
  EMBED_FRAME_ORIGINS,
  parsePeertubeHosts,
  renderCaddyfile,
  securityHeaders,
} from './csp';

const caddy = readFileSync(
  resolve(__dirname, '../../../../deploy/compose/caddy/Caddyfile.prod'),
  'utf8',
);

const table = JSON.parse(readFileSync(resolve(__dirname, '../../public-surfaces.json'), 'utf8'));
const webRs = readFileSync(resolve(__dirname, '../../../../crates/sr-server/src/web.rs'), 'utf8');
const rustConst = (name: string) =>
  new RegExp('pub const ' + name + ': &str = "([^"]*)";').exec(webRs)?.[1];

const snippet = readFileSync(
  resolve(__dirname, '../../../../deploy/compose/caddy/activities.snippet'),
  'utf8',
);
const robotsTxt = readFileSync(resolve(__dirname, '../../public/robots.txt'), 'utf8');

describe('CSP', () => {
  it('tells crawlers that only the landing page is indexable (OPS-IDX-001, D-384)', () => {
    expect(caddy).toContain('@notLanding not path /');
    expect(caddy).toContain('header @notLanding >X-Robots-Tag "noindex, nofollow"');
    expect(snippet).toContain('>X-Robots-Tag "noindex, nofollow"');
    expect(robotsTxt).toBe(
      ['User-agent: *', 'Allow: /$', 'Disallow: /', ''].join(String.fromCharCode(10)),
    );
    expect(robotsTxt).not.toMatch(/sitemap/i);
  });

  it('pins the public-surface block of the production Caddyfile (D-385)', () => {
    expect(caddy).toContain(renderSurfacesBlock(table));
    // One public policy on every bundle surface's shell; the app policy is not in the Caddyfile.
    const shells = table.surfaces.filter(
      (s: { kind: string; reserved?: boolean }) => s.kind === 'bundle' && !s.reserved,
    );
    expect(caddy.split(`header Content-Security-Policy "${PUBLIC_CSP}"`)).toHaveLength(
      shells.length + 1,
    );
    expect(caddy).not.toContain('wasm-unsafe-eval');
    expect(caddy).toContain(`Permissions-Policy "${PERMISSIONS_POLICY}"`);
  });

  it('keeps the public policy equal to the one sr-server sends (web.rs)', () => {
    expect(rustConst('PUBLIC_CSP')).toBe(PUBLIC_CSP);
    expect(rustConst('NO_SCRIPT_CSP')).toBe(NO_SCRIPT_CSP);
  });

  it('gives public shells no wasm, blob, frame or media allowance', () => {
    for (const csp of [PUBLIC_CSP, NO_SCRIPT_CSP]) {
      expect(csp).not.toMatch(/wasm-unsafe-eval|blob:|frame-src|media-src/);
      expect(csp).toContain("frame-ancestors 'none'");
    }
    expect(NO_SCRIPT_CSP).not.toContain('script-src');
    expect(NO_SCRIPT_CSP).not.toContain('connect-src');
  });

  it('names the report endpoint in report-uri and report-to, in the Caddyfile and the dev headers (RDY-SEC-009)', () => {
    expect(buildCsp().endsWith('; report-uri /internal/csp-report; report-to csp-endpoint')).toBe(
      true,
    );
    expect(caddy).toContain('header Reporting-Endpoints "csp-endpoint=\\"/internal/csp-report\\""');
    expect(caddy).toContain('path /internal/csp-report');
    for (const dev of [true, false]) {
      const h = securityHeaders({ dev });
      expect(h['Reporting-Endpoints']).toBe('csp-endpoint="/internal/csp-report"');
    }
    const vite = readFileSync(resolve(__dirname, '../../vite.config.ts'), 'utf8');
    expect(vite.split("'/internal/csp-report'")).toHaveLength(3);
  });

  it('allows WASM compilation but no inline or eval scripts', () => {
    const csp = buildCsp();
    expect(csp).toContain("script-src 'self' 'wasm-unsafe-eval';");
    expect(csp).not.toMatch(/script-src[^;]*'unsafe-(inline|eval)'/);
  });

  it('forbids framing and plugins, and keeps fonts and images same-origin', () => {
    const csp = buildCsp();
    expect(csp).toContain("frame-ancestors 'none'");
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("font-src 'self'");
    expect(csp).toMatch(/img-src 'self' data: blob:/);
  });

  it('appends extra origins and relaxes only for dev', () => {
    expect(buildCsp({ extraConnect: ['wss://voice.example.org'] })).toContain(
      "connect-src 'self' wss://voice.example.org",
    );
    expect(buildCsp({ extraFrame: ['https://*.activities.example.org'] })).toContain(
      "frame-src 'self' https://www.youtube-nocookie.com https://player.vimeo.com https://*.activities.example.org",
    );
    expect(buildCsp({ dev: true })).toContain("'unsafe-inline'");
    expect(buildCsp()).not.toMatch(/script-src[^;]*'unsafe-inline'/);
  });

  describe('privacy-enhanced embeds and direct media (PRF-UNF-001)', () => {
    const directive = (csp: string, name: string) =>
      csp
        .split('; ')
        .find((d) => d.startsWith(`${name} `))
        ?.split(' ')
        .slice(1);

    it('frames exactly the two provider embed hosts, and nothing else by default', () => {
      expect(directive(buildCsp(), 'frame-src')).toEqual([
        "'self'",
        'https://www.youtube-nocookie.com',
        'https://player.vimeo.com',
      ]);
      expect([...EMBED_FRAME_ORIGINS]).toEqual([
        'https://www.youtube-nocookie.com',
        'https://player.vimeo.com',
      ]);
      // Not the tracking-enabled hosts.
      expect(buildCsp()).not.toMatch(/https:\/\/(www\.)?youtube\.com|https:\/\/vimeo\.com/);
    });

    it('allows https media for the native player and no other new origin', () => {
      expect(directive(buildCsp(), 'media-src')).toEqual(["'self'", 'blob:', 'https:']);
      expect(directive(buildCsp(), 'img-src')).toEqual(["'self'", 'data:', 'blob:']);
      expect(directive(buildCsp(), 'connect-src')).toEqual(["'self'"]);
      expect(directive(buildCsp(), 'script-src')).toEqual(["'self'", "'wasm-unsafe-eval'"]);
    });

    it('never reaches the public or the no-script policy (D-385)', () => {
      for (const csp of [PUBLIC_CSP, NO_SCRIPT_CSP]) {
        expect(csp).not.toMatch(/youtube|vimeo|media-src|frame-src/);
      }
    });

    it('PeerTube instances are framed only when the operator names them', () => {
      expect(
        parsePeertubeHosts(' Tube.Example.org , https://a.example:443/ ,bad host,localhost'),
      ).toEqual(['tube.example.org', 'a.example']);
      const none = cspOptionsFromConfig({ publicUrl: 'https://chat.example.org' });
      expect(directive(buildCsp(none), 'frame-src')).not.toContain('https://tube.example.org');
      const some = cspOptionsFromConfig({
        publicUrl: 'https://chat.example.org',
        peertubeHosts: 'tube.example.org',
      });
      expect(directive(buildCsp(some), 'frame-src')).toContain('https://tube.example.org');
      expect(directive(buildCsp(some), 'connect-src')).toEqual(["'self'"]);
    });
  });

  it('dev headers are report-only so they can never break a flow; HSTS only on request', () => {
    const dev = securityHeaders({ dev: true });
    expect(dev['Content-Security-Policy']).toBeUndefined();
    expect(dev['Content-Security-Policy-Report-Only']).toBeDefined();
    expect(dev['Strict-Transport-Security']).toBeUndefined();
    expect(securityHeaders({ hsts: true })['Strict-Transport-Security']).toContain('max-age=');
  });

  describe('origins from config (H-27)', () => {
    const site = 'https://chat.example.org';

    it('activities off and voice on the same origin add nothing', () => {
      const opts = cspOptionsFromConfig({ publicUrl: site, voicePublicUrl: `${site}/voice` });
      expect(buildCsp(opts)).toBe(buildCsp());
      expect(buildCsp(cspOptionsFromConfig({ publicUrl: site, activitiesEnabled: false }))).toBe(
        buildCsp(),
      );
    });

    it('activities on lists the wildcard activities host in frame-src only', () => {
      const csp = buildCsp(cspOptionsFromConfig({ publicUrl: site, activitiesEnabled: true }));
      expect(csp).toContain(
        "frame-src 'self' https://www.youtube-nocookie.com https://player.vimeo.com https://*.activities.chat.example.org;",
      );
      expect(csp).toContain("connect-src 'self';");
    });

    it('a separate voice host is listed in connect-src, http and ws twins', () => {
      const csp = buildCsp(
        cspOptionsFromConfig({ publicUrl: site, voicePublicUrl: 'wss://voice.example.org:7443' }),
      );
      expect(csp).toContain(
        "connect-src 'self' https://voice.example.org:7443 wss://voice.example.org:7443;",
      );
    });

    it('ignores junk and non-https sites for activities', () => {
      expect(cspOptionsFromConfig({ publicUrl: 'nope', activitiesEnabled: true })).toEqual({
        extraFrame: [],
        extraConnect: [],
      });
      expect(
        cspOptionsFromConfig({ publicUrl: 'http://localhost:8080', activitiesEnabled: true })
          .extraFrame,
      ).toEqual([]);
    });

    it('renders the Caddyfile block from the surface table, and the committed file is its output', () => {
      expect(renderCaddyfile(caddy, table)).toBe(caddy);
      const moved = { ...table, publicFiles: [...table.publicFiles, '/extra.txt'] };
      expect(renderCaddyfile(caddy, moved)).toContain('/extra.txt');
      expect(renderCaddyfile(renderCaddyfile(caddy, moved), table)).toBe(caddy);
    });

    it('refuses a Caddyfile without the markers', () => {
      expect(() => renderCaddyfile('handle { }', table)).toThrow(/surfaces:begin/);
    });

    it('the deploy script --check agrees with the committed Caddyfile', () => {
      const script = resolve(__dirname, '../../../../deploy/scripts/render-caddyfile.mjs');
      expect(() =>
        execFileSync(process.execPath, [script, '--public-url', site, '--check'], {
          stdio: 'pipe',
        }),
      ).not.toThrow();
    });
  });
});
