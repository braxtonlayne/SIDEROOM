import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { buildCsp, PERMISSIONS_POLICY, securityHeaders } from './csp';

const caddy = readFileSync(
  resolve(__dirname, '../../../../deploy/compose/caddy/Caddyfile.prod'),
  'utf8',
);

describe('CSP', () => {
  it('matches the policy shipped in the production Caddyfile', () => {
    expect(caddy).toContain(`header Content-Security-Policy "${buildCsp()}"`);
    expect(caddy).toContain(`Permissions-Policy "${PERMISSIONS_POLICY}"`);
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
});
