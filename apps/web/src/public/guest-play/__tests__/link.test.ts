import { GAME_KEY } from '../contract';
import {
  decodeLobby,
  encodeDirect,
  encodeRelayed,
  encodeSys,
  unwrapFromRoom,
  wrapForRoom,
} from '../envelope';
import { buildGuestLink, generateGameKey, parseGuestLocation } from '../link';
import { importGameKey, open, seal } from '../seal';

const TOKEN = 'ab12'.repeat(16);

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
    expect(() =>
      buildGuestLink('https://x.test', TOKEN.toUpperCase(), generateGameKey()),
    ).toThrow();
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
    bent[bent.length - 1] = (bent[bent.length - 1] ?? 0) ^ 1;
    expect(await open(k, TOKEN, bent)).toBeNull();
    expect(await open(k, TOKEN, new Uint8Array(5))).toBeNull();
  });
});

describe('the lobby envelope', () => {
  const payload = new Uint8Array([7, 8, 9]);
  const list = (b: Uint8Array) => Array.from(b);

  it('a guest seat is g<seat>; a member seat relays under the room member id', () => {
    const g = decodeLobby(4, 'guest', encodeDirect(payload));
    expect(g).toMatchObject({ kind: 'data', from: 'g4' });
    expect(g && g.kind === 'data' ? list(g.bytes) : null).toEqual([7, 8, 9]);
    const m = decodeLobby(1, 'member', encodeRelayed('123456789', payload));
    expect(m).toMatchObject({ kind: 'data', from: '123456789' });
    expect(m && m.kind === 'data' ? list(m.bytes) : null).toEqual([7, 8, 9]);
  });

  it('a guest cannot pass as a member: a forged relayed or control payload is data from itself', () => {
    const forged = decodeLobby(4, 'guest', encodeRelayed('123456789', payload));
    expect(forged).toMatchObject({ kind: 'data', from: 'g4' });
    const sys = decodeLobby(4, 'guest', encodeSys({ host: '1' }));
    expect(sys?.kind).toBe('data');
  });

  it('reads the host a member seat names, and drops malformed control and empty payloads', () => {
    expect(decodeLobby(1, 'member', encodeSys({ host: '55' }))).toEqual({
      kind: 'sys',
      host: '55',
    });
    expect(decodeLobby(1, 'member', new Uint8Array([2, 1, 2, 3]))).toBeNull();
    expect(decodeLobby(1, 'member', new Uint8Array([1, 9, 1]))).toBeNull();
    expect(decodeLobby(1, 'member', new Uint8Array([9, 9]))).toBeNull();
    expect(decodeLobby(1, 'member', new Uint8Array(0))).toBeNull();
  });

  it('wraps a guest message for the other members of the room and unwraps only that', () => {
    const w = wrapForRoom('g4', payload);
    expect(unwrapFromRoom(w)).toMatchObject({ from: 'g4' });
    expect(list(unwrapFromRoom(w)!.bytes)).toEqual([7, 8, 9]);
    expect(unwrapFromRoom(new TextEncoder().encode('{"t":"state"}'))).toBeNull();
    expect(unwrapFromRoom(new Uint8Array([0xff, 0x53, 0x52, 0x47, 9, 1]))).toBeNull();
  });
});
