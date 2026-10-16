/**
 * VOX-E2E-002/006: the media key state a call's worker holds, over the sr-e2ee WASM API.
 *
 * Sending: a new epoch's encryptor is *prepared* when this client processes a commit, and only
 * *executed* (made current) when the server says everyone is ready (op 101 `execute`), so
 * receivers already hold the key when the first frame under it arrives. Receiving: the key ring
 * keeps the previous epoch for a grace window (10 s), so frames encrypted just before a sender
 * switched still play.
 *
 * Fail closed: without a key, or when encryption or decryption fails, the frame is dropped
 * (`null`). A plaintext frame is never sent or played.
 */
import type { E2eeApi, E2eeEncryptor, E2eeKeyRing } from '@sideroom/wasm';

export const GRACE_MS = 10_000;

export interface MediaStats {
  encrypted: number;
  decrypted: number;
  /** Frames dropped on the way out (no key yet, unsupported codec, error). */
  sendDropped: number;
  /** Frames dropped on the way in (no key, failed authentication). */
  recvDropped: number;
}

export class E2eeMedia {
  private current: { epoch: number; enc: E2eeEncryptor } | null = null;
  private pending = new Map<number, E2eeEncryptor>();
  private readonly ring: E2eeKeyRing;
  readonly stats: MediaStats = { encrypted: 0, decrypted: 0, sendDropped: 0, recvDropped: 0 };

  constructor(
    private readonly api: E2eeApi,
    private readonly random: (n: number) => Uint8Array = (n) =>
      crypto.getRandomValues(new Uint8Array(n)),
    graceMs = GRACE_MS,
  ) {
    this.ring = api.createKeyRing(graceMs);
  }

  /** The epoch frames are sent under, or -1 before the first. */
  get sendEpoch(): number {
    return this.current?.epoch ?? -1;
  }

  get receiveEpoch(): number {
    return this.ring.epoch;
  }

  /** This client's own base secret for `epoch` (from the MLS exporter). */
  prepareSend(epoch: number, secret: Uint8Array): void {
    if (epoch <= this.sendEpoch || this.pending.has(epoch)) return;
    this.pending.set(epoch, this.api.createEncryptor(secret, this.random(4)));
  }

  /** Switch to `epoch`'s encryptor (server `execute`). Older pending ones are dropped. */
  execute(epoch: number): boolean {
    const enc = this.pending.get(epoch);
    if (!enc) return false;
    for (const [e, p] of this.pending) {
      if (e <= epoch) {
        if (e !== epoch) p.free();
        this.pending.delete(e);
      }
    }
    this.current?.enc.free();
    this.current = { epoch, enc };
    return true;
  }

  /** Everyone's base secrets for `epoch`; one 16-byte secret per user id. */
  setReceive(epoch: number, userIds: string[], secrets: Uint8Array, nowMs: number): void {
    this.ring.setEpoch(epoch, userIds, secrets, nowMs);
  }

  encrypt(mime: string, frame: Uint8Array): Uint8Array | null {
    if (!this.current) {
      this.stats.sendDropped++;
      return null;
    }
    try {
      const out = this.current.enc.encrypt(mime, frame);
      this.stats.encrypted++;
      return out;
    } catch {
      this.stats.sendDropped++;
      return null;
    }
  }

  decrypt(sender: string, mime: string, frame: Uint8Array, nowMs: number): Uint8Array | null {
    try {
      const out = this.ring.decrypt(sender, mime, frame, nowMs);
      this.stats.decrypted++;
      return out;
    } catch {
      this.stats.recvDropped++;
      return null;
    }
  }

  /** Left the call: forget every key. */
  close(): void {
    this.current?.enc.free();
    this.current = null;
    for (const p of this.pending.values()) p.free();
    this.pending.clear();
    this.ring.clear();
  }
}
