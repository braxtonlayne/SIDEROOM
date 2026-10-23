import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import {
  buildCsp,
  cspOptionsFromConfig,
  PERMISSIONS_POLICY,
  renderCaddyfile,
  securityHeaders,
} from './csp';

const caddy = readFileSync(
  resolve(__dirname, '../../../../deploy/compose/caddy/Caddyfile.prod'),
  'utf8',
);

describe('CSP', () => {
  it('matches the policy shipped in the production Caddyfile', () => {
    expect(caddy).toContain(`header Content-Security-Policy "${buildCsp()}"`);
    expect(caddy).toContain(`Permissions-Policy "${PERMISSIONS_POLICY}"`);
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
      "frame-src 'self' https://*.activities.example.org",
    );
    expect(buildCsp({ dev: true })).toContain("'unsafe-inline'");
    expect(buildCsp()).not.toMatch(/script-src[^;]*'unsafe-inline'/);
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
      expect(csp).toContain("frame-src 'self' https://*.activities.chat.example.org;");
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

    it('renders the Caddyfile line from the same settings, and the committed file is the default', () => {
      expect(renderCaddyfile(caddy)).toBe(caddy);
      const on = renderCaddyfile(
        caddy,
        cspOptionsFromConfig({
          publicUrl: site,
          activitiesEnabled: true,
          voicePublicUrl: 'wss://voice.example.org',
        }),
      );
      expect(on).toContain("frame-src 'self' https://*.activities.chat.example.org;");
      expect(on).toContain("connect-src 'self' https://voice.example.org wss://voice.example.org;");
      expect(renderCaddyfile(on)).toBe(caddy);
    });

    it('the deploy script --check agrees with the committed Caddyfile', () => {
      const script = resolve(__dirname, '../../../../deploy/scripts/render-caddyfile.mjs');
      expect(() =>
        execFileSync(process.execPath, [script, '--public-url', site, '--check'], {
          stdio: 'pipe',
        }),
      ).not.toThrow();
      expect(() =>
        execFileSync(process.execPath, [script, '--public-url', site, '--activities', '--check'], {
          stdio: 'pipe',
        }),
      ).toThrow();
    });
  });
});
