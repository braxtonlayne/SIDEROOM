/**
 * VOX-E2E-001/004/006: one call's MLS session on the main thread, driven by op 101.
 *
 * - Joining: `hello`; the gateway answers `call` with the group id, its external-sender key and
 *   whether we create the group (exactly one member does). Everyone else sends a key package
 *   and waits for a welcome.
 * - Membership: key packages of people in the channel are added, members who left are removed
 *   (the server proposes it as external sender; the commit is still a member's). The member
 *   with the lowest leaf in the call commits at once; the others only if nothing happened
 *   after `FALLBACK_MS`. The server takes the first commit per epoch; a rejected one is dropped.
 * - Every epoch: each member's 16-byte sender secret (MLS exporter) goes to the media worker,
 *   which prepares the new keys, and we report `ready` with our leaf. The worker switches its
 *   encryptor on the server's `execute`; receivers keep the old keys for a grace window.
 * - Anything MLS refuses (a commit adding someone not in the call, a removal of someone still
 *   in it) puts us in `error` and we ask to be re-added with a fresh key package: never a
 *   silent fallback to plaintext.
 */
import type { E2eeApi, E2eeCallClient } from '@sideroom/wasm';
import type { WorkerIn } from './protocol';

export const FALLBACK_MS = 3_000;

export interface E2eeIdentity {
  /** 65-byte raw (uncompressed SEC1) P-256 identity public key. */
  rawPublicKey: Uint8Array;
  /** ECDSA P-256 / SHA-256 signature with the identity key (64 bytes r||s). */
  sign(message: Uint8Array): Promise<Uint8Array>;
}

export interface E2eeCallDeps {
  api: E2eeApi;
  me: string;
  channelId: string;
  identity: E2eeIdentity;
  /** Send op 101 (`channel_id` is added); false when the gateway is down. */
  send(d: Record<string, unknown>): boolean;
  /** User ids in the voice channel now, the viewer included (from the voice states). */
  roster(): string[];
  worker: { postMessage(m: WorkerIn, transfer?: Transferable[]): void };
  setTimer(fn: () => void, ms: number): unknown;
  clearTimer(handle: unknown): void;
}

export interface E2eeMember {
  userId: string;
  /** Hex of the 65-byte identity public key. */
  identityKey: string;
}

export interface E2eeCallState {
  status: 'starting' | 'waiting' | 'secure' | 'error';
  epoch: number;
  /** D-250 call code of the current epoch. */
  callCode: string | null;
  members: E2eeMember[];
  error: string | null;
}

interface CommitJson {
  based_on: number;
  commit: string;
  welcome: string | null;
  welcomed: string[];
}

export class E2eeCall {
  private client: E2eeCallClient | null = null;
  private keyPackages = new Map<string, string>();
  private pendingCommit = false;
  private proposals = false;
  private timer: unknown = null;
  private stopped = false;
  private listeners = new Set<(s: E2eeCallState) => void>();
  state: E2eeCallState = {
    status: 'starting',
    epoch: -1,
    callCode: null,
    members: [],
    error: null,
  };

  constructor(private readonly deps: E2eeCallDeps) {}

  subscribe(cb: (s: E2eeCallState) => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  private set(p: Partial<E2eeCallState>): void {
    this.state = { ...this.state, ...p };
    for (const l of this.listeners) l(this.state);
  }

  private send(d: Record<string, unknown>): void {
    this.deps.send({ ...d, channel_id: this.deps.channelId });
  }

  /** Certify a session key with the identity key, then say hello. */
  async start(): Promise<void> {
    const pending = this.deps.api.createPendingSession(
      this.deps.me,
      this.deps.identity.rawPublicKey,
    );
    const sig = await this.deps.identity.sign(pending.certMessage());
    if (this.stopped) {
      pending.free();
      return;
    }
    this.client = pending.certify(sig);
    pending.free();
    this.send({ type: 'hello' });
  }

  /** Say hello again (a resumed or new gateway session). */
  rehello(): void {
    if (this.client) this.send({ type: 'hello' });
  }

  stop(): void {
    this.stopped = true;
    this.deps.clearTimer(this.timer);
    this.client?.leave();
    this.client?.free();
    this.client = null;
    this.deps.worker.postMessage({ type: 'close' });
    this.listeners.clear();
  }

  private roster(): string[] {
    return this.deps.roster();
  }

  private members(): { leaf: number; user_id: string; identity_key: string }[] {
    if (!this.client?.hasGroup) return [];
    return JSON.parse(this.client.members()) as {
      leaf: number;
      user_id: string;
      identity_key: string;
    }[];
  }

  /** An `E2EE_MLS` dispatch for this channel. */
  onMessage(d: Record<string, unknown>): void {
    const c = this.client;
    if (!c || this.stopped || d.channel_id !== this.deps.channelId) return;
    const from = typeof d.from === 'string' ? d.from : null;
    const data = typeof d.data === 'string' ? d.data : '';
    try {
      switch (d.type) {
        case 'call':
          this.onCall(d);
          break;
        case 'key_package':
          if (from && from !== this.deps.me) {
            this.keyPackages.set(from, data);
            this.schedule();
          }
          break;
        case 'proposal':
          if (!c.hasGroup) break;
          if ((JSON.parse(c.handle(data, this.roster())) as { type: string }).type === 'proposal') {
            this.proposals = true;
            this.schedule();
          }
          break;
        case 'commit':
          this.onCommit(from, data);
          break;
        case 'commit_rejected':
          if (this.pendingCommit) {
            c.commitRejected();
            this.pendingCommit = false;
          }
          this.schedule();
          break;
        case 'welcome':
          c.join(data, this.roster());
          this.pendingCommit = false;
          this.onEpoch();
          break;
        case 'execute':
          if (typeof d.epoch === 'number')
            this.deps.worker.postMessage({ type: 'execute', epoch: d.epoch });
          break;
      }
    } catch (e) {
      this.recover(e);
    }
  }

  private onCall(d: Record<string, unknown>): void {
    const c = this.client!;
    const groupId = String(d.group_id ?? '');
    const ds = d.external_sender as { key?: string; identity?: string } | null | undefined;
    if (d.create === true) {
      c.leave();
      this.pendingCommit = false;
      c.createGroup(groupId, ds?.key ?? null, ds?.identity ?? null);
      this.onEpoch();
    } else {
      c.leave();
      this.set({ status: 'waiting', epoch: -1, callCode: null, members: [] });
      this.send({ type: 'key_package', data: c.keyPackage() });
    }
  }

  private onCommit(from: string | null, data: string): void {
    const c = this.client!;
    if (!c.hasGroup) return; // not in the group yet: our welcome follows
    if (from === this.deps.me && this.pendingCommit) {
      c.commitAccepted();
      this.pendingCommit = false;
      this.onEpoch();
      return;
    }
    const r = JSON.parse(c.handle(data, this.roster())) as { type: string };
    if (r.type === 'commit') {
      this.pendingCommit = false;
      this.onEpoch();
    }
  }

  /** A new epoch: hand its keys to the worker, report ready. */
  private onEpoch(): void {
    const c = this.client!;
    const epoch = c.epoch;
    const ids = c.senderUserIds();
    const secrets = c.senderSecrets();
    const mine = ids.indexOf(this.deps.me);
    if (mine >= 0) {
      const own = secrets.slice(mine * 16, mine * 16 + 16);
      this.deps.worker.postMessage({ type: 'prepareSend', epoch, secret: own }, [own.buffer]);
    }
    this.deps.worker.postMessage({ type: 'setReceive', epoch, userIds: ids, secrets }, [
      secrets.buffer,
    ]);
    if (ids.length === 1) {
      // Alone: nobody else needs the key first.
      this.deps.worker.postMessage({ type: 'execute', epoch });
    }
    this.send({ type: 'ready', epoch, leaf: c.ownLeaf() });
    const members = this.members().map((m) => ({ userId: m.user_id, identityKey: m.identity_key }));
    for (const m of members) this.keyPackages.delete(m.userId);
    this.proposals = false;
    this.set({
      status: 'secure',
      epoch,
      callCode: this.deps.api.callCode(c.epochAuthenticator()),
      members,
      error: null,
    });
    this.schedule();
  }

  /** The voice channel's occupants changed. */
  onRosterChanged(): void {
    const roster = new Set(this.roster());
    for (const u of [...this.keyPackages.keys()]) if (!roster.has(u)) this.keyPackages.delete(u);
    this.schedule();
  }

  private needsCommit(): boolean {
    if (!this.client?.hasGroup || this.pendingCommit) return false;
    const roster = new Set(this.roster());
    const members = this.members();
    const inGroup = new Set(members.map((m) => m.user_id));
    return (
      this.proposals ||
      [...this.keyPackages.keys()].some((u) => roster.has(u) && !inGroup.has(u)) ||
      members.some((m) => !roster.has(m.user_id))
    );
  }

  private isLeader(): boolean {
    const roster = new Set(this.roster());
    const here = this.members()
      .filter((m) => roster.has(m.user_id))
      .sort((a, b) => a.leaf - b.leaf);
    return here[0]?.user_id === this.deps.me;
  }

  private schedule(): void {
    this.deps.clearTimer(this.timer);
    this.timer = null;
    if (!this.needsCommit()) return;
    if (this.isLeader()) this.commit();
    else this.timer = this.deps.setTimer(() => this.commit(), FALLBACK_MS);
  }

  private commit(): void {
    const c = this.client;
    if (!c || this.stopped || !this.needsCommit()) return;
    try {
      const raw = c.commit([...this.keyPackages.values()], this.roster());
      const out = JSON.parse(raw) as CommitJson | null;
      if (!out) return;
      this.pendingCommit = true;
      this.send({
        type: 'commit',
        epoch: out.based_on,
        data: out.commit,
        ...(out.welcome ? { welcome: out.welcome, to: out.welcomed } : {}),
      });
    } catch (e) {
      this.recover(e);
    }
  }

  /** MLS refused something: leave our copy of the group and ask to be added again. */
  private recover(e: unknown): void {
    const c = this.client;
    if (!c || this.stopped) return;
    this.set({ status: 'error', error: e instanceof Error ? e.message : String(e) });
    c.leave();
    this.pendingCommit = false;
    try {
      this.send({ type: 'key_package', data: c.keyPackage() });
    } catch {
      // Nothing more to do; the error stays visible.
    }
  }
}
