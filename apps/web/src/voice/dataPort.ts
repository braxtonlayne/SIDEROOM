/**
 * ACT-CH-001: the client's end of the activity data channel. One SCTP data channel (`sr-act`) per call goes
 * to the SFU, which relays opaque bytes between the members who joined the same activity instance of the
 * room (crates/sr-voice/src/datachannel.rs has the framing). Every message is sealed by the E2EE worker
 * with the call's current sender key before it is written and opened there after it arrives, so the SFU
 * sees only ciphertext.
 *
 *   client -> SFU   kind(1) instance(8 BE) payload        kind 0 join, 1 leave, 2 data
 *   SFU -> client   kind(1)=2 instance(8) sender(8 BE) payload
 *
 * The port outlives engines: a rejoin builds a new peer connection and `attach` re-sends the joins.
 */
import type { DataChannelLike } from './engine';

export const LABEL = 'sr-act';
const JOIN = 0;
const LEAVE = 1;
const DATA = 2;
const HEADER_OUT = 17;

export interface DataSealer {
  sealData?(bytes: Uint8Array): Promise<Uint8Array | null>;
  openData?(sender: string, bytes: Uint8Array): Promise<Uint8Array | null>;
}

export type DataListener = (instance: string, sender: string, bytes: Uint8Array) => void;

function frame(kind: number, instance: string, payload?: Uint8Array): Uint8Array {
  const out = new Uint8Array(9 + (payload?.length ?? 0));
  out[0] = kind;
  new DataView(out.buffer).setBigUint64(1, BigInt(instance));
  if (payload) out.set(payload, 9);
  return out;
}

export class ActivityDataPort {
  private channel: DataChannelLike | null = null;
  private sealer: DataSealer | null = null;
  private readonly instances = new Set<string>();
  private readonly listeners = new Set<DataListener>();
  /** Sends and receives are chained so the order on the wire is the order of the calls. */
  private outChain: Promise<void> = Promise.resolve();
  private inChain: Promise<void> = Promise.resolve();

  /** True when messages can go out right now. */
  get open(): boolean {
    return this.channel?.readyState === 'open';
  }

  /** Bind a fresh data channel (a new peer connection). Joins are re-sent when it opens. */
  attach(channel: DataChannelLike, sealer: DataSealer): void {
    this.detach();
    this.channel = channel;
    this.sealer = sealer;
    channel.binaryType = 'arraybuffer';
    const sendJoins = () => {
      for (const i of this.instances) channel.send(frame(JOIN, i));
    };
    channel.onopen = sendJoins;
    channel.onmessage = (ev) => this.receive(ev.data);
    if (channel.readyState === 'open') sendJoins();
  }

  detach(): void {
    const c = this.channel;
    this.channel = null;
    this.sealer = null;
    if (c) {
      c.onopen = c.onclose = c.onmessage = null;
      try {
        c.close();
      } catch {
        /* already closed */
      }
    }
  }

  /** Start receiving an activity instance's messages (and let this client send into it). */
  join(instance: string): void {
    this.instances.add(instance);
    if (this.open) this.channel?.send(frame(JOIN, instance));
  }

  leave(instance: string): void {
    this.instances.delete(instance);
    if (this.open) this.channel?.send(frame(LEAVE, instance));
  }

  /** Seal and send one message to the other members of `instance`; false when it was dropped. */
  send(instance: string, bytes: Uint8Array): Promise<boolean> {
    const run = async (): Promise<boolean> => {
      const sealer = this.sealer;
      const ch = this.channel;
      if (!sealer?.sealData || !ch || ch.readyState !== 'open' || !this.instances.has(instance)) {
        return false;
      }
      const sealed = await sealer.sealData(bytes);
      if (!sealed || ch.readyState !== 'open') return false;
      ch.send(frame(DATA, instance, sealed));
      return true;
    };
    const p = this.outChain.then(run, run);
    this.outChain = p.then(
      () => undefined,
      () => undefined,
    );
    return p;
  }

  onMessage(cb: DataListener): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  private receive(data: unknown): void {
    if (!(data instanceof ArrayBuffer)) return;
    const b = new Uint8Array(data);
    if (b.length <= HEADER_OUT || b[0] !== DATA) return;
    const view = new DataView(data);
    const instance = view.getBigUint64(1).toString();
    const sender = view.getBigUint64(9).toString();
    if (!this.instances.has(instance)) return;
    const payload = b.slice(HEADER_OUT);
    const sealer = this.sealer;
    const run = async () => {
      const plain = await sealer?.openData?.(sender, payload);
      if (!plain) return;
      for (const l of [...this.listeners]) l(instance, sender, plain);
    };
    this.inChain = this.inChain.then(run, run);
  }
}

/** The one port of this window; the voice engine attaches the call's data channel to it. */
export const activityDataPort = new ActivityDataPort();
