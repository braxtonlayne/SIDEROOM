import { existsSync, readFileSync } from 'node:fs';
import { webcrypto } from 'node:crypto';
import { resolve } from 'node:path';
import { loadE2ee, type E2eeApi } from '@sideroom/wasm';
import { beforeAll, describe, expect, it } from 'vitest';
import { E2eeCall, FALLBACK_MS, type E2eeCallDeps } from '../e2ee/call';
import { handleWorkerMessage } from '../e2ee/handler';
import { loadOrCreateIdentity, memoryKeyStore, signWithIdentity } from '../e2ee/identity';
import { E2eeMedia } from '../e2ee/media';
import type { WorkerIn } from '../e2ee/protocol';

// VOX-E2E-001/003/004/006 end to end in one process: real MLS (sr-e2ee WASM) in each client's
// E2eeCall, a relay with the gateway's op 101 rules (first commit per epoch, welcomes to their
// recipients, execute when all are ready), and each client's media keys in a real E2eeMedia,
// so frames one member encrypts are checked to decrypt at the others. The gateway's own relay
// is tested in Rust (crates/sr-gateway/src/voice_e2ee_tests.rs), with the same message shapes.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/e2ee/e2ee_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const CH = '20';
const crypto = webcrypto as unknown as Crypto;

type Msg = Record<string, unknown>;

/** The gateway's op 101 rules, in miniature. */
class Relay {
  epoch = 0;
  creator: string | null = null;
  waiting: Set<string> | null = null;
  pendingEpoch = 0;
  roster = new Set<string>();
  members = new Map<string, Member>();
  log: Msg[] = [];

  deliver(to: string, d: Msg) {
    this.members.get(to)?.inbox.push({ ...d, channel_id: CH });
  }
  others(from: string, d: Msg) {
    for (const u of this.roster) if (u !== from) this.deliver(u, d);
  }
  all(d: Msg) {
    for (const u of this.roster) this.deliver(u, d);
  }
  receive(from: string, d: Msg) {
    this.log.push({ from, ...d });
    switch (d.type) {
      case 'hello': {
        const create = this.creator === null;
        if (create) this.creator = from;
        this.deliver(from, { type: 'call', group_id: btoa('call'), epoch: this.epoch, create });
        break;
      }
      case 'key_package':
        this.others(from, { type: 'key_package', from, data: d.data });
        break;
      case 'commit':
        if (d.epoch !== this.epoch) {
          this.deliver(from, { type: 'commit_rejected', epoch: this.epoch });
          break;
        }
        this.epoch += 1;
        this.all({ type: 'commit', from, epoch: this.epoch, data: d.data });
        if (typeof d.welcome === 'string')
          for (const u of (d.to as string[]) ?? [])
            if (u !== from && this.roster.has(u))
              this.deliver(u, { type: 'welcome', from, epoch: this.epoch, data: d.welcome });
        this.waiting = new Set([...this.roster].filter((u) => u !== from));
        this.pendingEpoch = this.epoch;
        if (this.waiting.size === 0) this.all({ type: 'execute', epoch: this.epoch });
        break;
      case 'ready':
        if (this.waiting && d.epoch === this.pendingEpoch) {
          this.waiting.delete(from);
          if (this.waiting.size === 0) {
            this.waiting = null;
            this.all({ type: 'execute', epoch: this.pendingEpoch });
          }
        }
        break;
    }
  }
  leave(u: string) {
    this.roster.delete(u);
    if (this.waiting?.delete(u) && this.waiting.size === 0) {
      this.waiting = null;
      this.all({ type: 'execute', epoch: this.pendingEpoch });
    }
    this.members.get(u)?.call.stop();
    this.members.delete(u);
    for (const m of this.members.values()) m.call.onRosterChanged();
  }
}

interface Member {
  id: string;
  call: E2eeCall;
  media: E2eeMedia;
  inbox: Msg[];
  timers: (() => void)[];
}

async function member(api: E2eeApi, relay: Relay, id: string): Promise<Member> {
  const identity = await loadOrCreateIdentity(id, memoryKeyStore(), crypto);
  const media = new E2eeMedia(api, () => Uint8Array.of(1, 2, 3, Number(id)));
  const m = { id, media, inbox: [], timers: [] } as unknown as Member;
  const deps: E2eeCallDeps = {
    api,
    me: id,
    channelId: CH,
    identity: {
      rawPublicKey: identity.rawPublicKey,
      sign: (msg) => signWithIdentity(identity, msg, crypto.subtle),
    },
    send: (d) => {
      relay.receive(id, d);
      return true;
    },
    roster: () => [...relay.roster],
    worker: {
      postMessage: (msg: WorkerIn) =>
        handleWorkerMessage(
          media,
          msg,
          () => {},
          () => 1000,
        ),
    },
    setTimer: (fn) => {
      m.timers.push(fn);
      return fn;
    },
    clearTimer: (h) => {
      m.timers = m.timers.filter((f) => f !== h);
    },
  };
  m.call = new E2eeCall(deps);
  relay.roster.add(id);
  relay.members.set(id, m);
  for (const other of relay.members.values()) if (other !== m) other.call.onRosterChanged();
  await m.call.start();
  return m;
}

/** Deliver everything queued until the network is quiet; fire fallback timers if asked. */
function settle(relay: Relay, fireTimers = false) {
  for (let round = 0; round < 200; round++) {
    let moved = false;
    for (const m of relay.members.values()) {
      while (m.inbox.length) {
        moved = true;
        m.call.onMessage(m.inbox.shift()!);
      }
    }
    if (!moved && fireTimers) {
      for (const m of relay.members.values()) {
        const due = m.timers.splice(0);
        for (const f of due) {
          moved = true;
          f();
        }
      }
    }
    if (!moved) return;
  }
  throw new Error('relay did not settle');
}

/** A frame from `a` plays at `b` (and only with the right keys). */
function hears(a: Member, b: Member): boolean {
  const enc = a.media.encrypt('audio/opus', Uint8Array.of(0xfc, 1, 2, 3));
  if (!enc) return false;
  const dec = b.media.decrypt(a.id, 'audio/opus', enc, 1000);
  return !!dec && Array.from(dec).join() === '252,1,2,3';
}

describe.runIf(built)('a call secured end to end (MLS in WASM, relay, media keys)', () => {
  let api: E2eeApi;
  beforeAll(async () => {
    api = await loadE2ee(readFileSync(wasmPath));
  });

  it('three people join, talk, one leaves; keys follow every step', async () => {
    const relay = new Relay();
    const a = await member(api, relay, '1');
    settle(relay);
    expect(a.call.state.status).toBe('secure');
    expect(a.call.state.epoch).toBe(0);
    expect(a.media.sendEpoch).toBe(0); // alone: executed at once

    const b = await member(api, relay, '2');
    settle(relay);
    const c = await member(api, relay, '3');
    settle(relay);
    for (const m of [a, b, c]) {
      expect(m.call.state.status).toBe('secure');
      expect(m.call.state.members.map((x) => x.userId).sort()).toEqual(['1', '2', '3']);
    }
    expect(relay.epoch).toBe(2);
    // Everyone switched on execute and hears everyone.
    for (const m of [a, b, c]) expect(m.media.sendEpoch).toBe(2);
    expect(hears(a, b) && hears(b, c) && hears(c, a)).toBe(true);
    // Same call code everywhere.
    expect(new Set([a, b, c].map((m) => m.call.state.callCode)).size).toBe(1);
    // The leader (lowest leaf, A) made every commit; nobody raced.
    expect(relay.log.filter((l) => l.type === 'commit').every((l) => l.from === '1')).toBe(true);

    // C leaves: A removes C; the old keys are gone for C's frames' future.
    relay.leave('3');
    settle(relay);
    expect(relay.epoch).toBe(3);
    for (const m of [a, b]) {
      expect(m.call.state.members.map((x) => x.userId).sort()).toEqual(['1', '2']);
      expect(m.media.sendEpoch).toBe(3);
    }
    expect(hears(a, b) && hears(b, a)).toBe(true);
    // C's last epoch-2 encryptor no longer reaches anyone once the grace window has passed.
    const stale = c.media.encrypt('audio/opus', Uint8Array.of(1));
    expect(stale).toBeNull(); // C's call stopped and wiped its keys
  });

  it('when the leader leaves, the next member commits after the fallback', async () => {
    const relay = new Relay();
    const a = await member(api, relay, '1');
    settle(relay);
    const b = await member(api, relay, '2');
    settle(relay);
    const c = await member(api, relay, '3');
    settle(relay);
    expect(relay.epoch).toBe(2);
    relay.leave('1');
    settle(relay, true);
    expect(relay.epoch).toBe(3);
    expect(b.call.state.members.map((x) => x.userId).sort()).toEqual(['2', '3']);
    expect(hears(b, c) && hears(c, b)).toBe(true);
    expect(a.call.state).toBeDefined();
    expect(FALLBACK_MS).toBeGreaterThan(0);
  });

  it('a commit adding someone who is not in the call is refused, and the refuser is re-added', async () => {
    const relay = new Relay();
    const a = await member(api, relay, '1');
    settle(relay);
    const b = await member(api, relay, '2');
    settle(relay);
    expect(hears(a, b)).toBe(true);
    // A malicious member (A) commits an add for an outsider's key package.
    const outsider = await loadOrCreateIdentity('9', memoryKeyStore(), crypto);
    const p = api.createPendingSession('9', outsider.rawPublicKey);
    const x = p.certify(await signWithIdentity(outsider, p.certMessage(), crypto.subtle));
    const client = (a.call as unknown as { client: { commit(k: string[], r: string[]): string } })
      .client;
    const bad = JSON.parse(client.commit([x.keyPackage()], ['1', '2', '9'])) as {
      based_on: number;
      commit: string;
    };
    relay.receive('1', { type: 'commit', epoch: bad.based_on, data: bad.commit });
    // B refuses: error, then asks to be added again with a new key package.
    const inbox = relay.members.get('2')!.inbox;
    const commit = inbox.find((m) => m.type === 'commit')!;
    inbox.length = 0;
    b.call.onMessage(commit);
    expect(b.call.state.status).toBe('error');
    expect(relay.log.at(-1)).toMatchObject({ from: '2', type: 'key_package' });
  });
});
