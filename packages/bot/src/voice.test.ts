// RDY-DEV-001: the bot SDK's end-to-end encryption half over the real sr-e2ee WASM bundle (the one the browser
// uses). Members are real `E2eeCall` state machines; the delivery service is a small relay that behaves like the
// gateway's op 101 (first commit per epoch wins, `ready`/`execute`, welcomes only to the named users). Needs the
// bundle: `pnpm --filter @sideroom/wasm build`. SR_WASM_REQUIRED=1 (the wasm and e2e workflows) fails instead of
// skipping when it is missing.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadE2ee, type E2eeApi } from '@sideroom/wasm';
import { E2eeCall, E2eeMedia, Identity, type Action } from './voice';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../../wasm/dist/e2ee/e2ee_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

const CHANNEL = '900';

interface Member {
  call: E2eeCall;
  media: E2eeMedia;
  executed: number[];
  failures: string[];
}

/** The delivery service: ordering and relaying, nothing else. */
class Relay {
  members = new Map<string, Member>();
  private epoch = 0;
  private ready = new Set<string>();
  private first = true;
  private queue: { to: string; d: Record<string, unknown> }[] = [];
  sent: { from: string; d: Record<string, unknown> }[] = [];
  now = 1_000;

  constructor(private readonly api: E2eeApi) {}

  roster(): Set<string> {
    return new Set(this.members.keys());
  }

  join(id: string): Member {
    const call = E2eeCall.create(this.api, id, CHANNEL, Identity.generate());
    const member: Member = { call, media: new E2eeMedia(this.api, id), executed: [], failures: [] };
    this.members.set(id, member);
    this.apply(id, [call.hello()]);
    for (const [other, m] of this.members) {
      if (other !== id) this.apply(other, m.call.rosterChanged(this.roster(), this.now));
    }
    this.run();
    return member;
  }

  private apply(id: string, actions: Action[]): void {
    const m = this.members.get(id);
    if (!m) return;
    for (const a of actions) {
      if (a.kind === 'keys') m.media.prepare(a.keys, this.now);
      else if (a.kind === 'execute') {
        if (m.media.execute(a.epoch)) m.executed.push(a.epoch);
      } else if (a.kind === 'failed') m.failures.push(a.reason);
      else this.deliver(id, a.d);
    }
  }

  private broadcast(d: Record<string, unknown>): void {
    for (const to of this.members.keys()) this.queue.push({ to, d });
  }

  private deliver(from: string, d: Record<string, unknown>): void {
    this.sent.push({ from, d });
    switch (d.type) {
      case 'hello': {
        const create = this.first;
        this.first = false;
        this.queue.push({
          to: from,
          d: {
            type: 'call',
            channel_id: CHANNEL,
            group_id: 'Z3JvdXAtaWQ=',
            epoch: this.epoch,
            create,
            external_sender: null,
          },
        });
        break;
      }
      case 'key_package':
        for (const to of this.members.keys()) {
          if (to !== from) this.queue.push({ to, d: { ...d, from } });
        }
        break;
      case 'commit': {
        if (d.epoch !== this.epoch) {
          this.queue.push({ to: from, d: { type: 'commit_rejected', channel_id: CHANNEL, epoch: this.epoch } });
          break;
        }
        this.epoch += 1;
        this.ready.clear();
        const welcomed = new Set((d.to as string[] | undefined) ?? []);
        for (const to of this.members.keys()) {
          this.queue.push({ to, d: { type: 'commit', channel_id: CHANNEL, data: d.data, from, epoch: this.epoch } });
          if (welcomed.has(to)) {
            this.queue.push({
              to,
              d: { type: 'welcome', channel_id: CHANNEL, data: d.welcome, from, epoch: this.epoch },
            });
          }
        }
        break;
      }
      case 'ready':
        if (d.epoch === this.epoch) {
          this.ready.add(from);
          if (this.ready.size === this.members.size) {
            this.broadcast({ type: 'execute', channel_id: CHANNEL, epoch: this.epoch });
          }
        }
        break;
    }
  }

  /** Deliver everything until the queue is empty. */
  run(): void {
    for (let i = 0; this.queue.length > 0 && i < 500; i++) {
      const { to, d } = this.queue.shift()!;
      const m = this.members.get(to);
      if (m) this.apply(to, m.call.handle(d, this.roster(), this.now));
    }
  }

  /** Let the fallback timers of members who are not the leader fire. */
  tick(ms: number): void {
    this.now += ms;
    for (const [id, m] of this.members) {
      this.apply(id, m.call.poll(this.roster(), this.now));
    }
    this.run();
  }
}

const frame = (n: number) => Uint8Array.from({ length: 60 }, (_, i) => (i < 4 ? n : 0x5a));

describe.runIf(built)('bots in voice, end-to-end encryption (RDY-DEV-001)', () => {
  it('derives the same keys for three members and round-trips frames both ways', async () => {
    const api = await loadE2ee(readFileSync(wasmPath));
    const relay = new Relay(api);
    const a = relay.join('1');
    expect(a.executed.length, 'alone: the creator may encrypt at once').toBe(1);
    const b = relay.join('2');
    relay.tick(10);
    const c = relay.join('3');
    relay.tick(10);

    const epoch = a.call.epoch();
    expect(epoch).not.toBeNull();
    expect(b.call.epoch()).toBe(epoch);
    expect(c.call.epoch()).toBe(epoch);
    expect([a, b, c].map((m) => m.call.members().length)).toEqual([3, 3, 3]);
    expect(a.failures.concat(b.failures, c.failures)).toEqual([]);
    expect(Buffer.from(a.call.epochAuthenticator()).equals(Buffer.from(c.call.epochAuthenticator()))).toBe(
      true,
    );
    for (const m of [a, b, c]) expect(m.media.sendingEpoch()).toBe(epoch);

    const sent = a.media.encrypt(frame(7));
    expect(sent).not.toEqual(frame(7));
    expect(b.media.decrypt('1', sent)).toEqual(frame(7));
    expect(c.media.decrypt('1', sent)).toEqual(frame(7));
    const back = c.media.encrypt(frame(9));
    expect(a.media.decrypt('3', back)).toEqual(frame(9));
    // A frame is only good for its sender: another user's key does not open it.
    expect(() => b.media.decrypt('3', sent)).toThrow();
  });

  it('never encrypts before the server executes the epoch', async () => {
    const api = await loadE2ee(readFileSync(wasmPath));
    const media = new E2eeMedia(api, '5');
    expect(media.sendingEpoch()).toBe(-1);
    expect(() => media.encrypt(frame(1))).toThrow(/no key/);
    expect(media.execute(3)).toBe(false);
  });

  it('refuses a message for another channel and recovers from garbage by asking to be added again', async () => {
    const api = await loadE2ee(readFileSync(wasmPath));
    const relay = new Relay(api);
    const a = relay.join('1');
    expect(a.call.handle({ type: 'commit', channel_id: 'other', data: 'AAAA' }, relay.roster(), 0)).toEqual(
      [],
    );
    const actions = a.call.handle(
      { type: 'commit', channel_id: CHANNEL, data: 'AAAA', from: '9' },
      relay.roster(),
      0,
    );
    expect(actions.some((x) => x.kind === 'failed')).toBe(true);
    expect(actions.some((x) => x.kind === 'send' && x.d.type === 'key_package')).toBe(true);
  });

  it('keeps an identity between runs and signs with it', () => {
    const id = Identity.generate();
    const same = Identity.fromSecret(id.secret());
    expect(Buffer.from(same.publicKey()).equals(Buffer.from(id.publicKey()))).toBe(true);
    expect(id.publicKey()).toHaveLength(65);
    expect(id.publicKey()[0]).toBe(4);
    expect(id.sign(new Uint8Array([1, 2, 3]))).toHaveLength(64);
    expect(() => Identity.fromSecret(new Uint8Array(31))).toThrow();
    expect(String(id)).toBe('Identity(..)');
  });
});

describe('call state that needs no bundle', () => {
  it('has the Rust names', async () => {
    const mod = await import('./voiceIndex');
    for (const name of [
      'E2eeCall',
      'E2eeMedia',
      'Identity',
      'VoiceLink',
      'VoiceSession',
      'VoiceServerUpdate',
      'voiceStateBody',
      'closeIsRetryable',
    ]) {
      expect(mod, name).toHaveProperty(name);
    }
  });
});
