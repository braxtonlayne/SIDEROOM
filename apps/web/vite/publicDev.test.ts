import { describe, expect, it } from 'vitest';
import {
  isNavigation,
  isPublicFile,
  loginLocation,
  matchSurface,
  pathMatches,
  safeNext,
  type SurfaceTable,
} from './publicDev';

const table: SurfaceTable = {
  appShell: 'app.html',
  surfaces: [
    { surface: 'landing', kind: 'bundle', shell: 'index.html', paths: ['/', '/join/*'] },
    {
      surface: 'auth',
      kind: 'bundle',
      shell: 'auth.html',
      paths: ['/login', '/login/*', '/register', '/reset/*'],
    },
    {
      surface: 'invite',
      kind: 'bundle',
      shell: 'invite.html',
      paths: ['/invite/*', '/no-invitation'],
    },
    { surface: 'profile', kind: 'bundle', shell: 'profile.html', paths: ['/~*'], reserved: true },
    { surface: 'legal', kind: 'html', paths: ['/house-notes'] },
  ],
  publicFiles: ['/entrance/*', '/sw.js', '/manifest.webmanifest'],
};

describe('publicDev: the surface table', () => {
  it('matches Caddy-style patterns', () => {
    expect(pathMatches('/login/*', '/login')).toBe(true);
    expect(pathMatches('/login/*', '/login/2fa')).toBe(true);
    expect(pathMatches('/login/*', '/loginx')).toBe(false);
    expect(pathMatches('/~*', '/~alex')).toBe(true);
    expect(pathMatches('/register', '/register/x')).toBe(false);
    expect(pathMatches('/', '/')).toBe(true);
  });

  it('finds the surface of a path and none for app paths', () => {
    expect(matchSurface(table, '/')?.surface).toBe('landing');
    expect(matchSurface(table, '/join/abcd')?.surface).toBe('landing');
    expect(matchSurface(table, '/login/link')?.surface).toBe('auth');
    expect(matchSurface(table, '/invite/x')?.surface).toBe('invite');
    expect(matchSurface(table, '/house-notes')?.kind).toBe('html');
    expect(matchSurface(table, '/channels')).toBeUndefined();
    expect(matchSurface(table, '/app')).toBeUndefined();
    expect(matchSurface(table, '/nonexistent-xyz')).toBeUndefined();
  });

  it('knows the public files', () => {
    expect(isPublicFile(table, '/entrance/assets/a.js')).toBe(true);
    expect(isPublicFile(table, '/sw.js')).toBe(true);
    expect(isPublicFile(table, '/app/assets/a.js')).toBe(false);
  });
});

describe('publicDev: next and navigation', () => {
  it('applies the server rule for next', () => {
    expect(safeNext('/channels/1')).toBe('/channels/1');
    expect(safeNext('/share?title=x')).toBe('/share?title=x');
    expect(safeNext('//evil.example')).toBeNull();
    expect(safeNext('https://evil.example')).toBeNull();
    expect(safeNext('/\\evil')).toBeNull();
    expect(safeNext('/a\nb')).toBeNull();
    expect(safeNext('/' + 'a'.repeat(3000))).toBeNull();
    expect(safeNext('channels')).toBeNull();
  });

  it('builds the login redirect', () => {
    expect(loginLocation('/channels?x=1')).toBe('/login?next=%2Fchannels%3Fx%3D1');
    expect(loginLocation('/app')).toBe('/login');
    expect(loginLocation('//evil')).toBe('/login');
  });

  it('recognises document navigations only', () => {
    const nav = (headers: Record<string, string>, method = 'GET') =>
      isNavigation({ method, headers });
    expect(nav({ 'sec-fetch-mode': 'navigate' })).toBe(true);
    expect(nav({ accept: 'text/html,application/xhtml+xml' })).toBe(true);
    expect(nav({ accept: '*/*' })).toBe(false);
    expect(nav({ accept: 'application/json' })).toBe(false);
    expect(nav({ 'sec-fetch-mode': 'navigate' }, 'POST')).toBe(false);
  });
});
