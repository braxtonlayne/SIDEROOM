import { existsSync, readFileSync } from 'node:fs';
import { webcrypto } from 'node:crypto';
import { resolve } from 'node:path';
import { loadE2ee, type E2eeApi } from '@sideroom/wasm';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { attachTransform, peerConfigFor } from '../e2ee/attach';
import { e2eeFlagEnabled } from '../e2ee/flag';
import { handleRtcTransform, handleWorkerMessage } from '../e2ee/handler';
import {
  loadOrCreateIdentity,
  memoryKeyStore,
  unwrapForExport,
  type KeyStore,
} from '../e2ee/identity';
import { E2eeMedia } from '../e2ee/media';
import { detectTransformMode } from '../e2ee/support';
import { runTransform, transformFrame, type EncodedFrameLike } from '../e2ee/transform';
import { keyFingerprint, verifiedStore } from '../e2ee/verified';

// VOX-E2E-002..007: the browser half of call E2EE. The frame transform runs the real sr-e2ee
// WASM (needs `pnpm --filter @sideroom/wasm build`); vitest runs with apps/web as the working
// directory.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/e2ee/e2ee_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const crypto = webcrypto as unknown as Crypto;
const secret = (b: number) => new Uint8Array(16).fill(b);
const frame = (bytes: number[], mime?: string): EncodedFrameLike => ({
  data: Uint8Array.from(bytes).buffer,
  getMetadata: () => (mime ? { mimeType: mime } : {}),
});

describe('feature flag and browser support', () => {
  it('voice.e2ee is off unless the build turns it on', () => {
    expect(e2eeFlagEnabled({})).toBe(false);
    expect(e2eeFlagEnabled({ VITE_SR_VOICE_E2EE: '0' })).toBe(false);
    expect(e2eeFlagEnabled({ VITE_SR_VOICE_E2EE: '1' })).toBe(true);
    expect(e2eeFlagEnabled()).toBe(false);
  });

  it('prefers RTCRtpScriptTransform, falls back to createEncodedStreams, else none', () => {
    class ST {}
    expect(detectTransformMode({ RTCRtpScriptTransform: ST })).toBe('script-transform');
    expect(
      detectTransformMode({ RTCRtpSender: { prototype: { createEncodedStreams() {} } } }),
    ).toBe('encoded-streams');
    expect(detectTransformMode({ RTCRtpSender: { prototype: {} } })).toBe('none');
    expect(detectTransformMode({})).toBe('none');
  });

  it('attaches through the right API', () => {
    const worker = { postMessage: vi.fn() };
    const opts = { direction: 'send' as const, mime: 'audio/opus' };
    class ST {
      constructor(
        readonly w: unknown,
        readonly o: unknown,
      ) {}
    }
    const sender: { transform?: unknown } = {};
    expect(attachTransform(sender, worker, opts, 'script-transform', ST)).toBe(true);
    expect(sender.transform).toBeInstanceOf(ST);
    expect((sender.transform as ST).o).toBe(opts);

    const streams = { readable: new ReadableStream(), writable: new WritableStream() };
    const legacy = { createEncodedStreams: () => streams };
    expect(attachTransform(legacy, worker, opts, 'encoded-streams', undefined)).toBe(true);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: 'stream', options: opts, ...streams }, [
      streams.readable,
      streams.writable,
    ]);
    expect(attachTransform({}, worker, opts, 'none', ST)).toBe(false);
    expect(attachTransform({}, worker, opts, 'encoded-streams', ST)).toBe(false);
    expect(peerConfigFor('encoded-streams')).toEqual({ encodedInsertableStreams: true });
    expect(peerConfigFor('script-transform')).toEqual({});
  });
});

describe.runIf(built)('frame transform over the WASM', () => {
  let api: E2eeApi;
  beforeAll(async () => {
    api = await loadE2ee(readFileSync(wasmPath));
  });
  const rnd = () => Uint8Array.of(1, 2, 3, 4);

  it('never sends before an epoch is executed, then round-trips every codec', () => {
    const tx = new E2eeMedia(api, rnd);
    const rx = new E2eeMedia(api, rnd);
    expect(tx.encrypt('audio/opus', Uint8Array.of(1))).toBeNull();
    tx.prepareSend(1, secret(7));
    // Prepared is not executed: still nothing goes out.
    expect(tx.encrypt('audio/opus', Uint8Array.of(1))).toBeNull();
    expect(tx.execute(1)).toBe(true);
    expect(tx.sendEpoch).toBe(1);
    rx.setReceive(1, ['42'], secret(7), 0);
    const cases: [string, number[]][] = [
      ['audio/opus', [0xfc, 1, 2, 3]],
      ['video/VP8', [0x10, 2, 0, 0x9d, 1, 0x2a, 0x80, 2, 0xe0, 1, 9, 9, 9]],
      ['video/H264', [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21, 0x7f, 0x55]],
    ];
    for (const [mime, bytes] of cases) {
      const enc = tx.encrypt(mime, Uint8Array.from(bytes));
      expect(enc).not.toBeNull();
      expect(Array.from(rx.decrypt('42', mime, enc!, 1) ?? [])).toEqual(bytes);
    }
    // Unknown codec: dropped, not sent in the clear.
    expect(tx.encrypt('video/AV1', Uint8Array.of(1))).toBeNull();
    expect(tx.stats.sendDropped).toBe(3);
    expect(tx.stats.encrypted).toBe(3);
  });

  it('switches senders on execute and keeps the old epoch for the grace window', () => {
    const tx = new E2eeMedia(api, rnd);
    const rx = new E2eeMedia(api, rnd, 1000);
    tx.prepareSend(1, secret(1));
    tx.execute(1);
    rx.setReceive(1, ['5'], secret(1), 0);
    tx.prepareSend(2, secret(2));
    rx.setReceive(2, ['5'], secret(2), 100);
    // Until execute, frames still go out under epoch 1, and the receiver still plays them.
    const before = tx.encrypt('audio/opus', Uint8Array.of(7))!;
    expect(rx.decrypt('5', 'audio/opus', before, 500)).not.toBeNull();
    expect(tx.execute(3)).toBe(false);
    expect(tx.execute(2)).toBe(true);
    const after = tx.encrypt('audio/opus', Uint8Array.of(8))!;
    expect(Array.from(rx.decrypt('5', 'audio/opus', after, 2000)!)).toEqual([8]);
    // Past the grace window the old epoch is gone.
    expect(rx.decrypt('5', 'audio/opus', before, 2000)).toBeNull();
    expect(rx.stats.recvDropped).toBe(1);
    tx.close();
    expect(tx.sendEpoch).toBe(-1);
    expect(tx.encrypt('audio/opus', Uint8Array.of(1))).toBeNull();
  });

  it('rewrites frame data, drops failures, and uses the metadata codec', async () => {
    const tx = new E2eeMedia(api, rnd);
    tx.prepareSend(1, secret(3));
    tx.execute(1);
    const rx = new E2eeMedia(api, rnd);
    rx.setReceive(1, ['9'], secret(3), 0);
    const f = frame([0x31, 2, 0, 5, 5, 5], 'video/VP8');
    expect(transformFrame(tx, { direction: 'send', mime: 'audio/opus' }, f)).toBe(true);
    expect(f.data.byteLength).toBeGreaterThan(6);
    // Decrypting as the fallback codec would fail; the metadata says VP8.
    const recv = { direction: 'recv' as const, mime: 'audio/opus', sender: '9' };
    expect(transformFrame(rx, recv, f)).toBe(true);
    expect(Array.from(new Uint8Array(f.data))).toEqual([0x31, 2, 0, 5, 5, 5]);
    // A plaintext frame arriving is dropped, and so is a recv transform with no sender.
    expect(transformFrame(rx, recv, f)).toBe(false);
    expect(transformFrame(rx, { direction: 'recv', mime: 'video/VP8' }, frame([1]))).toBe(false);

    // Through real streams: both frames go out encrypted...
    const out: EncodedFrameLike[] = [];
    const src = [frame([0xfc, 1]), frame([0xfc, 2])];
    await runTransform(
      tx,
      { direction: 'send', mime: 'audio/opus' },
      new ReadableStream({
        start(c) {
          src.forEach((s) => c.enqueue(s));
          c.close();
        },
      }),
      new WritableStream({ write: (x) => void out.push(x) }),
    );
    expect(out).toHaveLength(2);
    // ...and on the way in the undecryptable one is dropped.
    const back: EncodedFrameLike[] = [];
    await runTransform(
      rx,
      { direction: 'recv', mime: 'audio/opus', sender: '9' },
      new ReadableStream({
        start(c) {
          c.enqueue(out[0]!);
          c.enqueue(frame([0xfc, 3]));
          c.close();
        },
      }),
      new WritableStream({ write: (x) => void back.push(x) }),
    );
    expect(back.map((b) => Array.from(new Uint8Array(b.data)))).toEqual([[0xfc, 1]]);
  });

  it('the worker handler installs keys, wipes the copies it was sent, and reports epochs', () => {
    const media = new E2eeMedia(api, rnd);
    const post = vi.fn();
    const s = secret(4);
    handleWorkerMessage(media, { type: 'prepareSend', epoch: 1, secret: s }, post);
    expect(Array.from(s).every((b) => b === 0)).toBe(true);
    handleWorkerMessage(media, { type: 'execute', epoch: 1 }, post);
    const many = new Uint8Array(32).fill(5);
    handleWorkerMessage(
      media,
      { type: 'setReceive', epoch: 1, userIds: ['1', '2'], secrets: many },
      post,
      () => 0,
    );
    expect(Array.from(many).every((b) => b === 0)).toBe(true);
    expect(post).toHaveBeenLastCalledWith({ type: 'stats', sendEpoch: 1, receiveEpoch: 1 });
    handleWorkerMessage(media, { type: 'close' }, post);
    expect(post).toHaveBeenLastCalledWith({ type: 'stats', sendEpoch: -1, receiveEpoch: -1 });

    // A ScriptTransform without options passes nothing through.
    const cancel = vi.fn(() => Promise.resolve());
    handleRtcTransform(media, {
      readable: { cancel } as unknown as ReadableStream<EncodedFrameLike>,
      writable: new WritableStream(),
    });
    expect(cancel).toHaveBeenCalled();
  });
});

describe('identity keys (VOX-E2E-005)', () => {
  it('creates once, reloads the same key, and keeps every private key non-extractable', async () => {
    const store = memoryKeyStore();
    const a = await loadOrCreateIdentity('7', store, crypto);
    const b = await loadOrCreateIdentity('7', store, crypto);
    expect(Array.from(b.publicKey)).toEqual(Array.from(a.publicKey));
    expect(b.signingKey.extractable).toBe(false);
    await expect(crypto.subtle.exportKey('pkcs8', b.signingKey)).rejects.toThrow();
    const wrap = (await store.getWrappingKey())!;
    expect(wrap.extractable).toBe(false);
    await expect(crypto.subtle.exportKey('raw', wrap)).rejects.toThrow();
    const rec = (await store.getIdentity('7'))!;
    expect(rec.wrappedPrivateKey.byteLength).toBeGreaterThan(100);

    // The key signs, and the stored public key verifies it.
    const data = new TextEncoder().encode('hello');
    const algo = { name: 'ECDSA', hash: 'SHA-256' };
    const sig = await crypto.subtle.sign(algo, b.signingKey, data);
    const pub = await crypto.subtle.importKey(
      'spki',
      b.publicKey,
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    );
    expect(await crypto.subtle.verify(algo, pub, sig, data)).toBe(true);

    // The MLS client can get the PKCS#8 bytes on request.
    const pkcs8 = await unwrapForExport('7', store, crypto.subtle);
    expect(pkcs8!.byteLength).toBeGreaterThan(100);
    expect(await unwrapForExport('8', store, crypto.subtle)).toBeNull();
    // Another account on the device gets its own key.
    const other = await loadOrCreateIdentity('8', store, crypto);
    expect(Array.from(other.publicKey)).not.toEqual(Array.from(a.publicKey));
  });

  it('starts over when the wrapping key no longer opens the stored identity', async () => {
    const store = memoryKeyStore();
    const a = await loadOrCreateIdentity('7', store, crypto);
    // Site data partly cleared: the wrapping key is gone, the identity record is not.
    let replaced: CryptoKey | undefined;
    const lost: KeyStore = {
      ...store,
      getWrappingKey: () => Promise.resolve(replaced),
      putWrappingKey: (k) => {
        replaced = k;
        return Promise.resolve();
      },
    };
    const b = await loadOrCreateIdentity('7', lost, crypto);
    expect(Array.from(b.publicKey)).not.toEqual(Array.from(a.publicKey));
  });
});

describe('verified participants (VOX-E2E-007)', () => {
  it('ties a verification to the identity key', async () => {
    const mem = new Map<string, string>();
    const storage = {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => void mem.set(k, v),
    } as unknown as Storage;
    const store = verifiedStore(storage);
    const fp = await keyFingerprint(Uint8Array.of(1, 2, 3), crypto.subtle);
    expect(fp).toMatch(/^[0-9a-f]{64}$/);
    expect(store.isVerified('5', fp)).toBe(false);
    store.setVerified('5', fp, true);
    expect(store.isVerified('5', fp)).toBe(true);
    // A new key for the same user is not verified.
    expect(store.isVerified('5', 'other')).toBe(false);
    store.setVerified('5', fp, false);
    expect(store.isVerified('5', fp)).toBe(false);
    // Corrupt storage reads as nothing verified.
    mem.set('sr-e2ee-verified', '{nope');
    expect(store.isVerified('5', fp)).toBe(false);
  });
});
