import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadE2ee, type E2eeApi } from '@sideroom/wasm';
import { beforeAll, describe, expect, it } from 'vitest';
import { ActivityDataPort, type DataSealer } from '../dataPort';
import type { DataChannelLike } from '../engine';
import { E2eeMedia } from '../e2ee/media';

// ACT-CH-001: the client's data channel end, with the real sr-e2ee WASM sealing and opening and a fake SFU.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/e2ee/e2ee_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const secret = new Uint8Array(16).fill(7);

class FakeChannel implements DataChannelLike {
  binaryType = 'blob';
  readyState = 'connecting';
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  sent: Uint8Array[] = [];
  send(d: Uint8Array) {
    this.sent.push(d.slice());
  }
  close() {
    this.readyState = 'closed';
  }
  open() {
    this.readyState = 'open';
    this.onopen?.();
  }
}

/** The SFU's job in the SDK-facing framing: stamp the sender, deliver within the instance. */
function relay(from: FakeChannel, to: FakeChannel, sender: bigint) {
  for (const f of from.sent.splice(0)) {
    if (f[0] !== 2) continue;
    const out = new Uint8Array(17 + f.length - 9);
    out[0] = 2;
    out.set(f.subarray(1, 9), 1);
    new DataView(out.buffer).setBigUint64(9, sender);
    out.set(f.subarray(9), 17);
    to.onmessage?.({ data: out.buffer });
  }
}

describe.runIf(built)('activity data port', () => {
  let api: E2eeApi;
  beforeAll(async () => {
    api = await loadE2ee(readFileSync(wasmPath));
  });

  function media(sendAs: boolean): DataSealer & { m: E2eeMedia } {
    const m = new E2eeMedia(api, () => Uint8Array.of(1, 2, 3, 4));
    m.setReceive(1, ['42'], secret, 0);
    if (sendAs) {
      m.prepareSend(1, secret);
      m.execute(1);
    }
    return {
      m,
      sealData: async (b) => m.sealData(b),
      openData: async (s, b) => m.openData(s, b, 1),
    };
  }

  it('joins on open, seals what it sends and opens what it receives, in order', async () => {
    const a = new ActivityDataPort();
    const b = new ActivityDataPort();
    const ca = new FakeChannel();
    const cb = new FakeChannel();
    const ma = media(true);
    const mb = media(false);
    a.attach(ca, ma);
    b.attach(cb, mb);
    a.join('900');
    b.join('900');
    ca.open();
    cb.open();
    expect(ca.sent[0]).toEqual(Uint8Array.of(0, 0, 0, 0, 0, 0, 0, 3, 132)); // JOIN 900
    ca.sent.length = 0;

    const got: string[] = [];
    b.onMessage((i, s, bytes) => got.push(`${i}:${s}:${new TextDecoder().decode(bytes)}`));
    expect(await a.send('900', new TextEncoder().encode('one'))).toBe(true);
    expect(await a.send('900', new TextEncoder().encode('two'))).toBe(true);
    // Nothing readable left the client.
    const wire = ca.sent.map((f) => new TextDecoder('latin1').decode(f));
    expect(wire.join('')).not.toContain('one');
    relay(ca, cb, 42n);
    await new Promise((r) => setTimeout(r, 0));
    expect(got).toEqual(['900:42:one', '900:42:two']);

    // A replayed frame is dropped; an instance not joined is not sent or delivered.
    const again = ca.sent.length;
    expect(again).toBe(0);
    expect(await a.send('901', Uint8Array.of(1))).toBe(false);
  });

  it('drops a forged or replayed message and re-joins after a new channel', async () => {
    const b = new ActivityDataPort();
    const cb = new FakeChannel();
    b.attach(cb, media(false));
    b.join('5');
    cb.open();
    const got: Uint8Array[] = [];
    b.onMessage((_i, _s, bytes) => got.push(bytes));

    const tx = media(true);
    const sealed = tx.m.sealData(Uint8Array.of(9, 9))!;
    const frame = (payload: Uint8Array, sender: bigint) => {
      const out = new Uint8Array(17 + payload.length);
      out[0] = 2;
      new DataView(out.buffer).setBigUint64(1, 5n);
      new DataView(out.buffer).setBigUint64(9, sender);
      out.set(payload, 17);
      return out.buffer;
    };
    cb.onmessage?.({ data: frame(sealed, 42n) });
    cb.onmessage?.({ data: frame(sealed, 42n) }); // replay
    const forged = sealed.slice();
    forged[0] = (forged[0] ?? 0) ^ 1;
    cb.onmessage?.({ data: frame(forged, 42n) });
    cb.onmessage?.({ data: frame(sealed, 7n) }); // not a sender with a key
    await new Promise((r) => setTimeout(r, 0));
    expect(got).toEqual([Uint8Array.of(9, 9)]);

    // A new peer connection: the joins are sent again when its channel opens.
    const next = new FakeChannel();
    b.attach(next, media(false));
    next.open();
    expect(next.sent.length).toBe(1);
    expect(next.sent[0]?.[0]).toBe(0);
  });
});
