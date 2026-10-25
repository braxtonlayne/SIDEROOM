// Bots in voice, the end-to-end encryption half (RDY-DEV-001, D-249). The TypeScript twin of `sr_sdk::voice`
// (crates/sr-sdk/src/voice.rs): the same names and the same rules, over the `sr-e2ee-wasm` bundle the browser
// client uses (`@sideroom/wasm`, `loadE2ee`), so there is one MLS implementation.
//
// Voice and calls are end-to-end encrypted with MLS: one group per call, a per-epoch secret per sender and a
// per-frame AES-GCM transform the SFU never sees through. A bot that wants to hear or speak is a group member:
//
//  - `Identity`: the bot's persistent ECDSA P-256 identity key, which certifies each session's MLS key;
//  - `E2eeCall`: the op 101 state machine of one call. Feed it the `E2EE_MLS` dispatches and the roster; it
//    answers with `Action`s: payloads to send as gateway op 101, the sender keys of every epoch, and the
//    `execute` that says to switch encryptors;
//  - `E2eeMedia`: turns those keys into frame encryption (own frames) and decryption (everyone else's). It is a
//    `FrameTransform` of `voiceTransport.ts`.
//
// Sans-IO: no sockets and no clock of its own (callers pass millisecond times). Ids are strings (snowflakes do
// not fit a JavaScript number).
import { createECDH, createPrivateKey, generateKeyPairSync, sign as nodeSign } from 'node:crypto';
import type { E2eeApi, E2eeCallClient, E2eeEncryptor, E2eeKeyRing } from '@sideroom/wasm';

/** A member other than the call's leader waits this long for a commit that nobody made before making one. */
export const FALLBACK_MS = 3_000;
/** Receivers accept the previous epoch for this long after a switch. */
export const GRACE_MS = 10_000;
/** The codec of a voice frame, as the frame transform names it. */
export const OPUS = 'audio/opus';

/** What went wrong in a call's key agreement. */
export class VoiceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'VoiceError';
  }
}

/**
 * The bot's persistent identity key (ECDSA P-256). Keep its secret between runs: the other members see the key in
 * the privacy code of the call, and a bot whose key changes looks like a different bot.
 */
export class Identity {
  private constructor(
    private readonly secretBytes: Uint8Array,
    private readonly publicBytes: Uint8Array,
  ) {}

  /** A key from 32 secret bytes (a stored secret). */
  static fromSecret(secret: Uint8Array): Identity {
    if (secret.length !== 32) throw new VoiceError('identity key: the secret is 32 bytes');
    const ecdh = createECDH('prime256v1');
    try {
      ecdh.setPrivateKey(Buffer.from(secret));
    } catch (e) {
      throw new VoiceError(`identity key: ${e instanceof Error ? e.message : String(e)}`);
    }
    return new Identity(Uint8Array.from(secret), Uint8Array.from(ecdh.getPublicKey()));
  }

  /** A fresh key. */
  static generate(): Identity {
    const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
    const jwk = privateKey.export({ format: 'jwk' });
    return Identity.fromSecret(Buffer.from(jwk.d ?? '', 'base64url'));
  }

  /** The secret to store. */
  secret(): Uint8Array {
    return Uint8Array.from(this.secretBytes);
  }

  /** The 65-byte uncompressed SEC1 public key. */
  publicKey(): Uint8Array {
    return Uint8Array.from(this.publicBytes);
  }

  /** ECDSA P-256 / SHA-256, 64 bytes `r || s`. */
  sign(message: Uint8Array): Uint8Array {
    const x = Buffer.from(this.publicBytes.subarray(1, 33)).toString('base64url');
    const y = Buffer.from(this.publicBytes.subarray(33, 65)).toString('base64url');
    const d = Buffer.from(this.secretBytes).toString('base64url');
    const key = createPrivateKey({ key: { kty: 'EC', crv: 'P-256', x, y, d }, format: 'jwk' });
    return Uint8Array.from(nodeSign('sha256', message, { key, dsaEncoding: 'ieee-p1363' }));
  }

  toString(): string {
    return 'Identity(..)';
  }
}

/** The keys of one MLS epoch, ready for `E2eeMedia.prepare`. */
export interface EpochKeys {
  epoch: number;
  /** This member's leaf: what op 101 `ready` reports. */
  ownLeaf: number;
  /** Every member's 16-byte base secret for the epoch, by user id. */
  senders: { userId: string; secret: Uint8Array }[];
}

/** What the caller must do after feeding an `E2eeCall`. */
export type Action =
  /** Send this as the `d` of gateway op 101 (`channel_id` is already in it). */
  | { kind: 'send'; d: Record<string, unknown> }
  /** The group is at a new epoch: give the keys to `E2eeMedia.prepare`. The matching `ready` is queued. */
  | { kind: 'keys'; keys: EpochKeys }
  /** The server says everyone is ready (or timed out): switch with `E2eeMedia.execute`. */
  | { kind: 'execute'; epoch: number }
  /**
   * MLS refused something. The call asked to be added again with a fresh key package; do not send audio until
   * `keys` arrives, and never fall back to plaintext.
   */
  | { kind: 'failed'; reason: string };

interface CommitJson {
  based_on: number;
  commit: string;
  welcome: string | null;
  welcomed: string[];
}

const str = (d: Record<string, unknown>, k: string): string | undefined => {
  const v = d[k];
  return typeof v === 'string' ? v : undefined;
};

/** One call's MLS session, driven by op 101 (docs: `crates/sr-gateway/src/voice_e2ee.rs`). */
export class E2eeCall {
  private keyPackages = new Map<string, string>();
  private pendingCommit = false;
  private proposals = false;
  private due: number | null = null;

  private constructor(
    private readonly me: string,
    private readonly channelId: string,
    private readonly client: E2eeCallClient,
  ) {}

  /**
   * A session for the bot (user id `me`) in voice channel `channelId`. Certifies a fresh MLS key with the
   * identity key; send `hello()` next.
   */
  static create(api: E2eeApi, me: string, channelId: string, identity: Identity): E2eeCall {
    const pending = api.createPendingSession(me, identity.publicKey());
    try {
      const client = pending.certify(identity.sign(pending.certMessage()));
      return new E2eeCall(me, channelId, client);
    } catch (e) {
      throw new VoiceError(`mls: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      pending.free();
    }
  }

  private msg(d: Record<string, unknown>): Action {
    return { kind: 'send', d: { ...d, channel_id: this.channelId } };
  }

  /**
   * The `d` of the gateway op 4 that joins this call's channel (D-263). It declares `sr_e2ee: true`: with
   * `voice.e2ee` on the server refuses a join without it and removes a member who does not send `hello()` within
   * 10 seconds of `VOICE_SERVER_UPDATE`. `guildId` is null for a DM call.
   */
  joinState(guildId: string | null, selfMute = false, selfDeaf = false): Record<string, unknown> {
    return {
      guild_id: guildId,
      channel_id: this.channelId,
      self_mute: selfMute,
      self_deaf: selfDeaf,
      sr_e2ee: true,
    };
  }

  /** `{"type": "hello"}`: send on joining, and again on a new gateway session. */
  hello(): Action {
    return this.msg({ type: 'hello' });
  }

  /** The epoch the group is at, once it has one. */
  epoch(): number | null {
    return this.client.hasGroup ? this.client.epoch : null;
  }

  /** The call's privacy-code input: the epoch authenticator. */
  epochAuthenticator(): Uint8Array {
    return this.client.epochAuthenticator();
  }

  /** The members of the group with their verified identity keys (hex). */
  members(): { leaf: number; userId: string; identityKey: string }[] {
    if (!this.client.hasGroup) return [];
    return (
      JSON.parse(this.client.members()) as { leaf: number; user_id: string; identity_key: string }[]
    ).map((m) => ({ leaf: m.leaf, userId: m.user_id, identityKey: m.identity_key }));
  }

  /** The time (ms) `poll` wants to be called at, if a commit is due. */
  deadline(): number | null {
    return this.due;
  }

  /**
   * An `E2EE_MLS` dispatch for this channel (a dispatch for another channel is ignored). `roster` is the user
   * ids in the voice channel now, the bot included.
   */
  handle(d: Record<string, unknown>, roster: ReadonlySet<string>, nowMs: number): Action[] {
    if (str(d, 'channel_id') !== this.channelId) return [];
    const out: Action[] = [];
    try {
      this.dispatch(d, roster, nowMs, out);
    } catch (e) {
      this.recover(e, out);
    }
    return out;
  }

  private dispatch(
    d: Record<string, unknown>,
    roster: ReadonlySet<string>,
    nowMs: number,
    out: Action[],
  ): void {
    const c = this.client;
    const from = str(d, 'from') ?? null;
    const data = str(d, 'data') ?? '';
    const names = [...roster];
    switch (d.type) {
      case 'call':
        this.onCall(d, roster, nowMs, out);
        break;
      case 'key_package':
        if (from && from !== this.me) {
          this.keyPackages.set(from, data);
          this.schedule(roster, nowMs, out);
        }
        break;
      case 'proposal':
        if (!c.hasGroup) break;
        if ((JSON.parse(c.handle(data, names)) as { type: string }).type === 'proposal') {
          this.proposals = true;
          this.schedule(roster, nowMs, out);
        }
        break;
      case 'commit':
        this.onCommit(from, data, roster, nowMs, out);
        break;
      case 'commit_rejected':
        if (this.pendingCommit) {
          c.commitRejected();
          this.pendingCommit = false;
        }
        this.schedule(roster, nowMs, out);
        break;
      case 'welcome':
        c.join(data, names);
        this.pendingCommit = false;
        this.onEpoch(roster, nowMs, out);
        break;
      case 'execute':
        if (typeof d.epoch === 'number') out.push({ kind: 'execute', epoch: d.epoch });
        break;
    }
  }

  private onCall(
    d: Record<string, unknown>,
    roster: ReadonlySet<string>,
    nowMs: number,
    out: Action[],
  ): void {
    const c = this.client;
    const groupId = str(d, 'group_id') ?? '';
    const ds = d.external_sender as { key?: string; identity?: string } | null | undefined;
    if (d.create === true) {
      c.leave();
      this.pendingCommit = false;
      c.createGroup(groupId, ds?.key ?? null, ds?.identity ?? null);
      this.onEpoch(roster, nowMs, out);
    } else {
      c.leave();
      out.push(this.msg({ type: 'key_package', data: c.keyPackage() }));
    }
  }

  private onCommit(
    from: string | null,
    data: string,
    roster: ReadonlySet<string>,
    nowMs: number,
    out: Action[],
  ): void {
    const c = this.client;
    if (!c.hasGroup) return; // not in the group yet: our welcome follows
    if (from === this.me && this.pendingCommit) {
      c.commitAccepted();
      this.pendingCommit = false;
      this.onEpoch(roster, nowMs, out);
      return;
    }
    const r = JSON.parse(c.handle(data, [...roster])) as { type: string };
    if (r.type === 'commit') {
      this.pendingCommit = false;
      this.onEpoch(roster, nowMs, out);
    }
  }

  /** A new epoch: hand its keys over and report ready. */
  private onEpoch(roster: ReadonlySet<string>, nowMs: number, out: Action[]): void {
    const c = this.client;
    const epoch = c.epoch;
    const ids = c.senderUserIds();
    const secrets = c.senderSecrets();
    const senders = ids.map((userId, i) => ({
      userId,
      secret: secrets.slice(i * 16, i * 16 + 16),
    }));
    const ownLeaf = c.ownLeaf();
    out.push({ kind: 'keys', keys: { epoch, ownLeaf, senders } });
    if (ids.length === 1) {
      // Alone: nobody else needs the key first.
      out.push({ kind: 'execute', epoch });
    }
    out.push(this.msg({ type: 'ready', epoch, leaf: ownLeaf }));
    for (const m of this.members()) this.keyPackages.delete(m.userId);
    this.proposals = false;
    this.schedule(roster, nowMs, out);
  }

  /** The voice channel's occupants changed. */
  rosterChanged(roster: ReadonlySet<string>, nowMs: number): Action[] {
    for (const u of [...this.keyPackages.keys()]) if (!roster.has(u)) this.keyPackages.delete(u);
    const out: Action[] = [];
    try {
      this.schedule(roster, nowMs, out);
    } catch (e) {
      this.recover(e, out);
    }
    return out;
  }

  /** Call when `deadline()` has passed: a commit nobody else made is made now. */
  poll(roster: ReadonlySet<string>, nowMs: number): Action[] {
    const out: Action[] = [];
    if (this.due !== null && nowMs >= this.due) {
      this.due = null;
      try {
        this.commit(roster, out);
      } catch (e) {
        this.recover(e, out);
      }
    }
    return out;
  }

  private needsCommit(roster: ReadonlySet<string>): boolean {
    if (!this.client.hasGroup || this.pendingCommit) return false;
    const members = this.members();
    const inGroup = new Set(members.map((m) => m.userId));
    return (
      this.proposals ||
      [...this.keyPackages.keys()].some((u) => roster.has(u) && !inGroup.has(u)) ||
      members.some((m) => !roster.has(m.userId))
    );
  }

  /** The member with the lowest leaf among those still in the channel commits at once. */
  private isLeader(roster: ReadonlySet<string>): boolean {
    const here = this.members()
      .filter((m) => roster.has(m.userId))
      .sort((a, b) => a.leaf - b.leaf);
    return here[0]?.userId === this.me;
  }

  private schedule(roster: ReadonlySet<string>, nowMs: number, out: Action[]): void {
    this.due = null;
    if (!this.needsCommit(roster)) return;
    if (this.isLeader(roster)) this.commit(roster, out);
    else this.due = nowMs + FALLBACK_MS;
  }

  private commit(roster: ReadonlySet<string>, out: Action[]): void {
    if (!this.needsCommit(roster)) return;
    const raw = this.client.commit([...this.keyPackages.values()], [...roster]);
    const c = JSON.parse(raw) as CommitJson | null;
    if (!c) return;
    this.pendingCommit = true;
    out.push(
      this.msg({
        type: 'commit',
        epoch: c.based_on,
        data: c.commit,
        ...(c.welcome ? { welcome: c.welcome, to: c.welcomed } : {}),
      }),
    );
  }

  /** MLS refused something: leave our copy of the group and ask to be added again. */
  private recover(e: unknown, out: Action[]): void {
    this.client.leave();
    this.pendingCommit = false;
    this.due = null;
    out.push({ kind: 'failed', reason: e instanceof Error ? e.message : String(e) });
    try {
      out.push(this.msg({ type: 'key_package', data: this.client.keyPackage() }));
    } catch {
      // Nothing more to do; the failure is reported.
    }
  }

  /** Leave the group (left the channel). */
  leave(): void {
    this.client.leave();
    this.due = null;
    this.keyPackages.clear();
  }

  /** Free the wasm state. */
  free(): void {
    this.client.free();
  }
}

/**
 * Frame encryption for the bot's own audio and decryption of everyone else's, following the epochs an
 * `E2eeCall` reports. The encryptor of a new epoch is prepared when its keys arrive and used from the server's
 * `execute` on; receivers accept the previous epoch for a grace window. Fails closed: without a key a frame is
 * refused, never sent or played in the clear.
 */
export class E2eeMedia {
  private active: { epoch: number; enc: E2eeEncryptor } | null = null;
  private pending = new Map<number, E2eeEncryptor>();
  private readonly ring: E2eeKeyRing;

  /** `random` returns `n` random bytes (the 4-byte instance id of each encryptor). */
  constructor(
    private readonly api: E2eeApi,
    private readonly me: string,
    private readonly random: (n: number) => Uint8Array = (n) =>
      crypto.getRandomValues(new Uint8Array(n)),
    graceMs = GRACE_MS,
  ) {
    this.ring = api.createKeyRing(graceMs);
  }

  /** Install the keys of a new epoch: receiving works at once, sending after `execute`. */
  prepare(keys: EpochKeys, nowMs = Date.now()): void {
    const mine = keys.senders.find((s) => s.userId === this.me);
    if (mine && keys.epoch > this.sendingEpoch() && !this.pending.has(keys.epoch)) {
      this.pending.set(keys.epoch, this.api.createEncryptor(mine.secret, this.random(4)));
    }
    const secrets = new Uint8Array(keys.senders.length * 16);
    keys.senders.forEach((s, i) => secrets.set(s.secret, i * 16));
    this.ring.setEpoch(
      keys.epoch,
      keys.senders.map((s) => s.userId),
      secrets,
      nowMs,
    );
  }

  /** Switch the encryptor to `epoch` (the server's `execute`). False when those keys were never prepared. */
  execute(epoch: number): boolean {
    const enc = this.pending.get(epoch);
    if (!enc) return false;
    for (const [e, p] of this.pending) {
      if (e <= epoch) {
        if (e !== epoch) p.free();
        this.pending.delete(e);
      }
    }
    this.active?.enc.free();
    this.active = { epoch, enc };
    return true;
  }

  /** The epoch frames are being encrypted under, or -1 before the first `execute`. */
  sendingEpoch(): number {
    return this.active?.epoch ?? -1;
  }

  /** Encrypt one encoded frame; throws before the first `execute` (never send audio in the clear). */
  encrypt(frame: Uint8Array, mime = OPUS): Uint8Array {
    if (!this.active) throw new VoiceError('frame: no key');
    return this.active.enc.encrypt(mime, frame);
  }

  /** Decrypt a frame received from `sender`; throws when it must be dropped. */
  decrypt(sender: string, frame: Uint8Array, mime = OPUS, nowMs = Date.now()): Uint8Array {
    return this.ring.decrypt(sender, mime, frame, nowMs);
  }

  /** Forget every key (left the call). */
  clear(): void {
    this.active?.enc.free();
    this.active = null;
    for (const p of this.pending.values()) p.free();
    this.pending.clear();
    this.ring.clear();
  }
}
