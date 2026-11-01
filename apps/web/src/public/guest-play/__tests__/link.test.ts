import { cleanNickname, randomCallsign } from '../callsign';
import { GAME_KEY } from '../contract';
import { buildGuestLink, generateGameKey, parseGuestLocation } from '../link';
import { importGameKey, open, seal } from '../seal';

const TOKEN = 'Tk3-aB_9xQ0pLmN7vR2sYw';

describe('the Share link', () => {
  it('makes a 32-byte url-safe key in the browser, a new one each time', () => {
    const a = generateGameKey();
    expect(a).toMatch(GAME_KEY);
    expect(generateGameKey()).not.toBe(a);
  });

  it('puts the key in the fragment, which a browser never sends to a server', () => {
    const key = generateGameKey();
    const href = buildGuestLink('https://chat.example.org/', TOKEN, key);
    expect(href).toBe(`https://chat.example.org/play/${TOKEN}#key=${key}`);
    const u = new URL(href);
    expect(u.pathname).toBe(`/play/${TOKEN}`);
    expect(u.search).toBe('');
    // What a request line carries: path and query. The key is in neither.
    expect(u.pathname + u.search).not.toContain(key);
  });

  it('refuses a token or key of the wrong shape', () => {
    expect(() => buildGuestLink('https://x.test', 'short', generateGameKey())).toThrow();
    expect(() => buildGuestLink('https://x.test', TOKEN, 'nope')).toThrow();
  });

  it('parses a location; a missing or malformed key is reported as null, a bad token as no link', () => {
    const key = generateGameKey();
    expect(parseGuestLocation(`/play/${TOKEN}`, `#key=${key}`)).toEqual({ token: TOKEN, key });
    expect(parseGuestLocation(`/play/${TOKEN}/`, `key=${key}`)).toEqual({ token: TOKEN, key });
    expect(parseGuestLocation(`/play/${TOKEN}`, '')).toEqual({ token: TOKEN, key: null });
    expect(parseGuestLocation(`/play/${TOKEN}`, '#key=short')?.key).toBeNull();
    expect(parseGuestLocation('/play/x', `#key=${key}`)).toBeNull();
    expect(parseGuestLocation('/play/%E0%A4%A', `#key=${key}`)).toBeNull();
    expect(parseGuestLocation('/channels/1', '')).toBeNull();
  });
});

describe('sealing with the game key', () => {
  it('round-trips, and the relay-visible bytes are not the plaintext', async () => {
    const k = await importGameKey(generateGameKey());
    const plain = new TextEncoder().encode('{"hp":3}');
    const sealed = await seal(k, TOKEN, plain);
    expect(sealed.length).toBe(12 + plain.length + 16);
    expect(new TextDecoder().decode(sealed)).not.toContain('hp');
    expect(Array.from((await open(k, TOKEN, sealed))!)).toEqual(Array.from(plain));
  });

  it('opens only under the same key and for the same lobby', async () => {
    const k = await importGameKey(generateGameKey());
    const other = await importGameKey(generateGameKey());
    const sealed = await seal(k, TOKEN, new Uint8Array([1, 2, 3]));
    expect(await open(other, TOKEN, sealed)).toBeNull();
    expect(await open(k, `${TOKEN}-other-lobby`, sealed)).toBeNull();
    const bent = sealed.slice();
    bent[bent.length - 1] ^= 1;
    expect(await open(k, TOKEN, bent)).toBeNull();
    expect(await open(k, TOKEN, new Uint8Array(5))).toBeNull();
  });
});

describe('callsigns and nicknames', () => {
  it('draws an adjective, a noun and a number', () => {
    expect(randomCallsign(() => 7)).toMatch(/^[A-Z][a-z]+ [A-Z][a-z]+ \d\d$/);
    expect(randomCallsign()).toMatch(/^[A-Z][a-z]+ [A-Z][a-z]+ \d\d$/);
  });

  it('cleans a nickname and refuses markup, control characters and long text', () => {
    expect(cleanNickname('  Mo   Money ')).toBe('Mo Money');
    expect(cleanNickname('')).toBeNull();
    expect(cleanNickname('<b>x</b>')).toBeNull();
    expect(cleanNickname('a'.repeat(25))).toBeNull();
    expect(cleanNickname('Zoë_9')).toBe('Zoë_9');
  });
});
