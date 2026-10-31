import { create } from 'zustand';
import type { HostableGame } from './transport';

/**
 * The seam to the emulator loader (ACT-EMU-001, apps/web/src/emu/*). When a core is running in this window,
 * the loader calls `offerCoopGame({ title, setInput })`; "Play together" in the voice panel then offers to host
 * it. When the game is closed the loader calls `withdrawCoopGame()`. Nothing here imports the loader, so the
 * co-op code stays small and the loader stays lazy (D-385: nothing emulator-related before sign-in).
 */
interface SourceState {
  game: HostableGame | null;
}

export const useCoopSource = create<SourceState>(() => ({ game: null }));

export function offerCoopGame(game: HostableGame): void {
  useCoopSource.setState({ game });
}

export function withdrawCoopGame(game?: HostableGame): void {
  // A game that is not the current offer cannot withdraw it (a second tab's loader closing late).
  if (game && useCoopSource.getState().game !== game) return;
  useCoopSource.setState({ game: null });
}
