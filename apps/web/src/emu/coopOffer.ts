// ACT-EMU-001 / ACT-EMU-002 seam. The co-op code (apps/web/src/emu/coop/*, wave 37 emu-coop, merged separately) exposes
// `offerCoopGame({ title, setInput(port, button, value) })` and `withdrawCoopGame()` in `coop/source.ts`; the running game
// calls them so "Play together" in a voice channel can host it. This file reaches them through a glob so the shell builds
// with or without the co-op branch merged: when `coop/source.ts` is absent the glob is empty and offering does nothing.
// Port is 0-based and `button` is the RetroPad index 0..15, the same numbers `simulate_input` takes.
import { retroIndex } from './gamepad';
import type { Button } from './gamepad';
import type { EmuSession } from './session';

export interface CoopGame {
  title: string;
  setInput(port: number, button: number, value: 0 | 1): void;
}

interface CoopSourceModule {
  offerCoopGame(game: CoopGame): void;
  withdrawCoopGame(game?: CoopGame): void;
}

const sources = import.meta.glob<CoopSourceModule>('./coop/source.ts');

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
  const load = sources['./coop/source.ts'];
  if (!load) return false;
  (await load()).offerCoopGame(game);
  return true;
}

export async function withdrawCoop(game: CoopGame): Promise<void> {
  const load = sources['./coop/source.ts'];
  if (load) (await load()).withdrawCoopGame(game);
}
