import {
  buildAuthorizeUrl,
  policyUrlError,
  isSafeRedirect,
  parseScopes,
  permissionBits,
  permissionIds,
  permissionLabels,
  redirectUriError,
} from '../oauth';
import { validateCommand } from '../tabs/Commands';
import { parseTab, validateAuthorizeSearch } from '../search';

describe('redirectUriError', () => {
  it.each([
    ['https://example.com/cb', null],
    ['http://localhost:3000/cb', null],
    ['http://127.0.0.1/cb', null],
  ])('accepts %s', (u, want) => expect(redirectUriError(u)).toBe(want));

  it.each([
    ['', /Enter a URL/],
    ['not a url', /spaces/],
    ['example.com/cb', /full URL/],
    ['https://*.example.com/cb', /Wildcards/],
    ['http://example.com/cb', /https/],
    ['https://example.com/cb#frag', /fragment/],
    ['https://user:pw@example.com/cb', /credentials/],
    ['javascript:alert(1)', /Only https/],
    ['data:text/html,hi', /Only https/],
    ['myapp://callback', /Only https/],
  ])('rejects %s', (u, msg) => expect(redirectUriError(u)).toMatch(msg));
});

describe('policyUrlError', () => {
  it('allows empty (clearing) and https only', () => {
    expect(policyUrlError('')).toBeNull();
    expect(policyUrlError('https://bot.example/privacy')).toBeNull();
    expect(policyUrlError('http://bot.example/privacy')).toMatch(/https/);
    expect(policyUrlError('nope')).toMatch(/full URL/);
    expect(policyUrlError(`https://x.example/${'a'.repeat(520)}`)).toMatch(/too long/);
  });
});

describe('permissions', () => {
  it('computes the Discord-compatible integer', () => {
    // SEND_MESSAGES (bit 11) + VIEW_CHANNEL (bit 10)
    expect(permissionBits(['SEND_MESSAGES', 'VIEW_CHANNEL'])).toBe((1n << 11n) | (1n << 10n));
    expect(permissionBits(['ADMINISTRATOR'])).toBe(8n);
    expect(permissionBits(['NOT_A_PERMISSION'])).toBe(0n);
  });

  it('round-trips ids and labels', () => {
    const bits = permissionBits(['SEND_MESSAGES', 'VIEW_CHANNEL']);
    expect(permissionIds(bits).sort()).toEqual(['SEND_MESSAGES', 'VIEW_CHANNEL']);
    expect(permissionLabels(bits.toString())).toEqual(
      expect.arrayContaining(['Send Messages', 'View Channels']),
    );
    expect(permissionLabels(null)).toEqual([]);
  });
});

describe('buildAuthorizeUrl', () => {
  it('builds an install link with permissions only for the bot scope', () => {
    const url = new URL(
      buildAuthorizeUrl({
        origin: 'https://sr.test/',
        clientId: '500',
        scopes: ['bot', 'applications.commands'],
        permissions: 2048n,
      }),
    );
    expect(url.pathname).toBe('/oauth2/authorize');
    expect(url.searchParams.get('client_id')).toBe('500');
    expect(url.searchParams.get('scope')).toBe('bot applications.commands');
    expect(url.searchParams.get('permissions')).toBe('2048');
    expect(url.searchParams.has('redirect_uri')).toBe(false);
  });

  it('adds response_type with a redirect and omits permissions without the bot scope', () => {
    const url = new URL(
      buildAuthorizeUrl({
        origin: 'https://sr.test',
        clientId: '500',
        scopes: ['identify'],
        redirectUri: 'https://example.com/cb',
        permissions: 8n,
        state: 'xyz',
      }),
    );
    expect(url.searchParams.get('response_type')).toBe('code');
    expect(url.searchParams.get('redirect_uri')).toBe('https://example.com/cb');
    expect(url.searchParams.has('permissions')).toBe(false);
    expect(url.searchParams.get('state')).toBe('xyz');
  });
});

describe('isSafeRedirect', () => {
  it('only follows the registered redirect', () => {
    const reg = 'https://example.com/cb';
    expect(isSafeRedirect('https://example.com/cb?code=1&state=2', reg)).toBe(true);
    expect(isSafeRedirect('https://evil.test/cb?code=1', reg)).toBe(false);
    expect(isSafeRedirect('https://example.com/other', reg)).toBe(false);
    expect(isSafeRedirect('javascript:alert(1)', null)).toBe(false);
    expect(isSafeRedirect('https://anywhere.test/', null)).toBe(true);
  });
});

describe('parsers', () => {
  it('splits scopes on spaces, plus and commas without duplicates', () => {
    expect(parseScopes('identify  guilds+bot,bot')).toEqual(['identify', 'guilds', 'bot']);
    expect(parseScopes(undefined)).toEqual([]);
  });
  it('keeps only known authorize params as strings', () => {
    expect(validateAuthorizeSearch({ client_id: 500, scope: 'bot', junk: 'x' })).toEqual({
      client_id: '500',
      scope: 'bot',
    });
    expect(parseTab('bot')).toBe('bot');
    expect(parseTab('nope')).toBe('general');
  });
});

describe('validateCommand', () => {
  const ok = { type: 1 as const, name: 'ping', description: 'Pong', options: [] };
  it('accepts a valid slash command', () => expect(validateCommand(ok)).toEqual({}));
  it('requires lowercase names and a description for slash commands', () => {
    expect(validateCommand({ ...ok, name: 'Ping' }).name).toBeDefined();
    expect(validateCommand({ ...ok, name: 'a b' }).name).toBeDefined();
    expect(validateCommand({ ...ok, description: '' }).description).toBeDefined();
  });
  it('allows a free-form name and no description for context menu commands', () => {
    expect(
      validateCommand({ type: 2, name: 'Show profile', description: '', options: [] }),
    ).toEqual({});
  });
  it('checks options: unique names, required before optional', () => {
    const o = (name: string, required: boolean) => ({
      type: 3,
      name,
      description: 'd',
      required,
    });
    const errs = validateCommand({ ...ok, options: [o('a', false), o('a', true)] });
    expect(errs['option.1.name']).toMatch(/unique/);
    expect(errs['option.1.required']).toMatch(/Required options must come before/);
  });
});
