// The player side: reads the public table from shared state, keeps this player's own hole cards (sent by the
// host to this member alone) and sends their choices to the host.

import type { Card } from './cards';
import type { Action } from './engine';
import {
  TABLE_KEY,
  decodeHostMsg,
  encode,
  type ClientMsg,
  type PublicPlayer,
  type PublicTable,
} from './protocol';
import type { Transport } from './transport';

export interface PlayerSnapshot {
  table: PublicTable | null;
  /** My hole cards for the hand `holeHand`, if the host has sent them. */
  hole: [Card, Card] | null;
  holeHand: number | null;
  error: string | null;
}

export class PlayerClient {
  private table: PublicTable | null = null;
  private hole: [Card, Card] | null = null;
  private holeHand: number | null = null;
  private error: string | null = null;
  private syncedHand = -1;
  private readonly listeners = new Set<() => void>();
  private readonly offs: (() => void)[] = [];

  constructor(private readonly tp: Transport) {}

  start(): void {
    this.offs.push(this.tp.onState(() => this.readState()));
    this.offs.push(
      this.tp.onInput((e) => {
        if (!e.reply) return; // inputs are for the host's dealer
        const msg = decodeHostMsg(e.data);
        if (!msg) return;
        if (msg.t === 'hole') {
          this.hole = msg.cards;
          this.holeHand = msg.hand;
          this.error = null;
        } else this.error = msg.message;
        this.emit();
      }),
    );
    this.readState();
    // A late joiner or a reload asks for their cards once.
    this.send({ t: 'sync' });
  }

  stop(): void {
    for (const off of this.offs.splice(0)) off();
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  snapshot(): PlayerSnapshot {
    return { table: this.table, hole: this.hole, holeHand: this.holeHand, error: this.error };
  }

  get self(): string {
    return this.tp.self;
  }

  /** My entry in the running hand, if I am dealt in. */
  me(): PublicPlayer | undefined {
    return this.table?.hand?.players.find((p) => p.id === this.tp.self);
  }

  /** My hole cards for the hand now on the table, or null (not dealt in, or not received yet). */
  myCards(): [Card, Card] | null {
    const hand = this.table?.hand;
    return hand && this.holeHand === hand.no ? this.hole : null;
  }

  // ---- what I can do ----

  sit(name: string, seat?: number): void {
    this.send({ t: 'sit', name, ...(seat === undefined ? {} : { seat }) });
  }
  stand(): void {
    this.send({ t: 'stand' });
  }
  rebuy(): void {
    this.send({ t: 'rebuy' });
  }
  act(action: Action): void {
    const hand = this.table?.hand;
    if (!hand) return;
    this.send({ t: 'act', hand: hand.no, action });
  }
  sync(): void {
    this.send({ t: 'sync' });
  }

  private send(msg: ClientMsg): void {
    this.tp.sendToHost(encode(msg));
  }

  private readState(): void {
    const t = this.tp.getState<PublicTable>(TABLE_KEY);
    if (!t || (this.table && t.rev < this.table.rev)) return;
    this.table = t;
    const hand = t.hand;
    // Dealt in to a hand but no cards for it yet (a message was missed): ask once per hand.
    if (
      hand &&
      !hand.over &&
      this.holeHand !== hand.no &&
      this.syncedHand !== hand.no &&
      hand.players.some((p) => p.id === this.tp.self)
    ) {
      this.syncedHand = hand.no;
      this.sync();
    }
    this.emit();
  }

  private emit(): void {
    for (const l of [...this.listeners]) l();
  }
}
