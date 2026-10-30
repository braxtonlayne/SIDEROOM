import { APP_DOOR, enterApp, exitToLogin, nav, safeNext } from './enterApp';

describe('safeNext (the server applies the same rule, crates/sr-server web.rs)', () => {
  it.each([
    ['/channels/1/2', '/channels/1/2'],
    ['/invite/abc?x=1', '/invite/abc?x=1'],
    ['/', '/'],
  ])('keeps %s', (input, out) => expect(safeNext(input)).toBe(out));

  it.each([
    ['//evil.example'],
    ['https://evil.example'],
    ['javascript:alert(1)'],
    ['/\\evil.example'],
    ['/a\\b'],
    ['/a\u0000b'],
    ['/a\nb'],
    ['relative/path'],
    [''],
    ['/' + 'a'.repeat(2048)],
  ])('refuses %j', (input) => expect(safeNext(input)).toBeNull());

  it('refuses a missing value', () => {
    expect(safeNext(undefined)).toBeNull();
    expect(safeNext(null)).toBeNull();
  });
});

describe('enterApp and exitToLogin', () => {
  beforeEach(() => {
    vi.spyOn(nav, 'assign').mockImplementation(() => {});
  });
  afterEach(() => vi.restoreAllMocks());

  it('goes to the neutral door by default and to a validated next otherwise', () => {
    enterApp();
    expect(nav.assign).toHaveBeenLastCalledWith(APP_DOOR);
    enterApp('/invite/abc');
    expect(nav.assign).toHaveBeenLastCalledWith('/invite/abc');
    enterApp('//evil.example');
    expect(nav.assign).toHaveBeenLastCalledWith(APP_DOOR);
  });

  it('leaves for sign-in with the current page as next', () => {
    exitToLogin('/library?x=1');
    expect(nav.assign).toHaveBeenLastCalledWith('/login?next=%2Flibrary%3Fx%3D1');
    exitToLogin('//evil');
    expect(nav.assign).toHaveBeenLastCalledWith('/login');
  });
});
