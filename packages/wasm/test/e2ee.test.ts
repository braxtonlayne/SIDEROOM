import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { webcrypto } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { loadE2ee } from '../src/index.js';

/**
 * VOX-E2E-008: Rust <-> WASM interop. crates/sr-e2ee/tests/vectors.json is written and checked
 * by the native crate; here the WASM build must produce the same ciphertexts byte for byte and
 * decrypt them back. Needs `pnpm build` first. Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail
 * instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/e2ee/e2ee_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

interface Vectors {
  secret: string;
  instance: string;
  frames: { codec: string; plaintext: string; ciphertext: string }[];
  pairwise: { user_a: string; key_a: string; user_b: string; key_b: string; code: string }[];
  call: { epoch_authenticator: string; code: string }[];
}

const vectors = JSON.parse(
  readFileSync(join(here, '../../../crates/sr-e2ee/tests/vectors.json'), 'utf8'),
) as Vectors;

const hex = (s: string) => Uint8Array.from(s.match(/../g) ?? [], (b) => parseInt(b, 16));
const toHex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');

describe.runIf(built)('sr-e2ee-wasm interop', () => {
  it('encrypts every vector exactly like the native crate', async () => {
    const e2ee = await loadE2ee(readFileSync(wasmPath));
    for (const f of vectors.frames) {
      // A fresh encryptor per vector (counter 1), with the vectors' instance id.
      const enc = e2ee.createEncryptor(hex(vectors.secret), hex(vectors.instance));
      expect(toHex(enc.encrypt(f.codec, hex(f.plaintext)))).toBe(f.ciphertext);
      expect(enc.counter).toBe(1);
      enc.free();
    }
  });

  it('decrypts the native ciphertexts and refuses tampered or plaintext frames', async () => {
    const e2ee = await loadE2ee(readFileSync(wasmPath));
    const ring = e2ee.createKeyRing(1000);
    expect(ring.epoch).toBe(-1);
    ring.setEpoch(1, ['175928847299117063'], hex(vectors.secret), 0);
    for (const f of vectors.frames) {
      const plain = ring.decrypt('175928847299117063', f.codec, hex(f.ciphertext), 1);
      expect(toHex(plain)).toBe(f.plaintext);
      const bad = hex(f.ciphertext);
      bad[0] = bad[0]! ^ 0x80;
      expect(() => ring.decrypt('175928847299117063', f.codec, bad, 1)).toThrow();
      // A plaintext frame is never passed through.
      expect(() => ring.decrypt('175928847299117063', f.codec, hex(f.plaintext), 1)).toThrow();
    }
    // Another sender has no key.
    expect(() => ring.decrypt('1', 'audio/opus', hex(vectors.frames[1]!.ciphertext), 1)).toThrow();
    ring.free();
  });

  it('keeps the previous epoch for the grace window only', async () => {
    const e2ee = await loadE2ee(readFileSync(wasmPath));
    const ring = e2ee.createKeyRing(1000);
    const oldSecret = new Uint8Array(16).fill(1);
    const newSecret = new Uint8Array(16).fill(2);
    const rnd = Uint8Array.of(9, 9, 9, 9);
    const oldTx = e2ee.createEncryptor(oldSecret, rnd);
    ring.setEpoch(1, ['5'], oldSecret, 0);
    ring.setEpoch(2, ['5'], newSecret, 100);
    expect(ring.epoch).toBe(2);
    const late = oldTx.encrypt('audio/opus', Uint8Array.of(1, 2, 3));
    expect(Array.from(ring.decrypt('5', 'audio/opus', late, 1099))).toEqual([1, 2, 3]);
    const tooLate = oldTx.encrypt('audio/opus', Uint8Array.of(1, 2, 3));
    expect(() => ring.decrypt('5', 'audio/opus', tooLate, 1100)).toThrow();
    expect(() => ring.setEpoch(2, [], new Uint8Array(0), 2000)).toThrow();
  });

  it('matches the native privacy codes (D-250) and refuses unknown codecs', async () => {
    const e2ee = await loadE2ee(readFileSync(wasmPath));
    for (const p of vectors.pairwise) {
      expect(e2ee.pairwiseCode(p.user_a, hex(p.key_a), p.user_b, hex(p.key_b))).toBe(p.code);
      expect(e2ee.pairwiseCode(p.user_b, hex(p.key_b), p.user_a, hex(p.key_a))).toBe(p.code);
    }
    for (const c of vectors.call) {
      expect(e2ee.callCode(hex(c.epoch_authenticator))).toBe(c.code);
    }
    expect(e2ee.isSupportedCodec('video/H264')).toBe(true);
    expect(e2ee.isSupportedCodec('video/AV1')).toBe(false);
    const enc = e2ee.createEncryptor(new Uint8Array(16), Uint8Array.of(1, 2, 3, 4));
    expect(() => enc.encrypt('video/AV1', Uint8Array.of(1))).toThrow();
    expect(() => e2ee.createEncryptor(new Uint8Array(15), Uint8Array.of(1, 2, 3, 4))).toThrow();
  });

  it('runs MLS in WASM: certify, create, add, join, same sender secrets, remove', async () => {
    const e2ee = await loadE2ee(readFileSync(wasmPath));
    const subtle = webcrypto.subtle;
    const user = async (id: string) => {
      const ik = await subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, [
        'sign',
        'verify',
      ]);
      const pub = new Uint8Array(await subtle.exportKey('raw', ik.publicKey));
      const pending = e2ee.createPendingSession(id, pub);
      const sig = new Uint8Array(
        await subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, ik.privateKey, pending.certMessage()),
      );
      return { id, pub, client: pending.certify(sig) };
    };
    const a = await user('1');
    const b = await user('2');
    const c = await user('3');
    a.client.createGroup(btoa('call-1'));
    expect(a.client.epoch).toBe(0);
    let roster = ['1', '2', '3'];
    const out = JSON.parse(
      a.client.commit([b.client.keyPackage(), c.client.keyPackage()], roster),
    ) as { based_on: number; commit: string; welcome: string; welcomed: string[] };
    expect(out.based_on).toBe(0);
    expect(out.welcomed).toEqual(['2', '3']);
    expect(a.client.commitAccepted()).toBe(1);
    expect(b.client.join(out.welcome, roster)).toBe(1);
    expect(c.client.join(out.welcome, roster)).toBe(1);
    const sec = (x: typeof a) => Array.from(x.client.senderSecrets()).join(',');
    expect(sec(b)).toBe(sec(a));
    expect(sec(c)).toBe(sec(a));
    expect(a.client.senderUserIds()).toEqual(['1', '2', '3']);
    expect(Array.from(b.client.epochAuthenticator())).toEqual(
      Array.from(a.client.epochAuthenticator()),
    );
    const members = JSON.parse(b.client.members()) as { user_id: string; identity_key: string }[];
    expect(members.find((m) => m.user_id === '1')!.identity_key).toBe(toHex(a.pub));

    // C leaves; B commits the removal, A follows.
    roster = ['1', '2'];
    const rm = JSON.parse(b.client.commit([], roster)) as {
      commit: string;
      welcome: string | null;
    };
    expect(rm.welcome).toBeNull();
    expect(b.client.commitAccepted()).toBe(2);
    expect(JSON.parse(a.client.handle(rm.commit, roster))).toEqual({ type: 'commit', epoch: 2 });
    expect(sec(a)).toBe(sec(b));
    expect(a.client.senderUserIds()).toEqual(['1', '2']);
    // A commit removing someone still in the call is refused.
    const kick = JSON.parse(a.client.commit([], ['1'])) as { commit: string };
    expect(() => b.client.handle(kick.commit, roster)).toThrow();
    // A forged certificate is refused.
    const p = e2ee.createPendingSession('4', a.pub);
    expect(() => p.certify(new Uint8Array(64))).toThrow();
  });
});
