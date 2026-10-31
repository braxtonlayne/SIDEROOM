import { activityDataPort, type ActivityDataPort } from '../../voice/dataPort';
import { COOP_INSTANCE } from './protocol';

/** What co-op needs from the call: the authenticated sender of each packet, and a way to broadcast. */
export interface CoopTransport {
  /** Start listening to (and sending into) the co-op instance of this call. */
  join(): void;
  leave(): void;
  /** Broadcast to everyone in the co-op instance; false when dropped (channel closed, budget). */
  send(bytes: Uint8Array): Promise<boolean>;
  onMessage(cb: (sender: string, bytes: Uint8Array) => void): () => void;
}

/** The real transport: the activity data channel of the call, instance `COOP_INSTANCE`. */
export function dataPortTransport(port: ActivityDataPort = activityDataPort): CoopTransport {
  return {
    join: () => port.join(COOP_INSTANCE),
    leave: () => port.leave(COOP_INSTANCE),
    send: (bytes) => port.send(COOP_INSTANCE, bytes),
    onMessage: (cb) =>
      port.onMessage((instance, sender, bytes) => {
        if (instance === COOP_INSTANCE) cb(sender, bytes);
      }),
  };
}

/** A running core that takes input for a port. The emulator loader's core object implements this. */
export interface CoopCore {
  /** Press (1) or release (0) RetroPad button `button` (see input.ts) of player `port` (0 = player 1). */
  setInput(port: number, button: number, value: 0 | 1): void;
}

/** What the host offers: a core and the name shown to guests. */
export interface HostableGame extends CoopCore {
  title: string;
}
