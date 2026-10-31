// ACT-EMU-001 / ACT-EMU-002 seam. The running game offers itself to the co-op code (`coop/source.ts`) so "Play together"
// in a voice channel can host it, and withdraws when it stops. `coop/source.ts` is loaded lazily (it is in the emulator
// chunk's graph only through this dynamic import). Port is 0-based and `button` is the RetroPad index 0..15, the same
// numbers `simulate_input` takes.
import { retroIndex } from './gamepad';
import type { Button } from './gamepad';
import type { EmuSession } from './session';

export interface CoopGame {
  title: string;
  setInput(port: number, button: number, value: 0 | 1): void;
}

const BY_INDEX: readonly Button[] = [
  'b',
  'y',
  'select',
  'start',
  'up',
  'down',
  'left',
  'right',
  'a',
  'x',
  'l',
  'r',
  'l2',
  'r2',
  'l3',
  'r3',
];

/** The game object the co-op code drives: pad inputs of remote players go straight to the session. */
export function coopGameFor(title: string, session: EmuSession): CoopGame {
  return {
    title,
    setInput: (port, button, value) => {
      const b = BY_INDEX[button];
      if (b === undefined || retroIndex(b) !== button) return;
      session.setButton(port, b, value === 1);
    },
  };
}

export async function offerCoop(game: CoopGame): Promise<boolean> {
  (await import('./coop/source')).offerCoopGame(game);
  return true;
}

export async function withdrawCoop(game: CoopGame): Promise<void> {
  (await import('./coop/source')).withdrawCoopGame(game);
}
