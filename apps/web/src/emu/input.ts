// ACT-EMU-001: turns keyboard and gamepad state into `setButton` calls, one poll at a time. The keyboard drives
// player 1 together with the first pad; the next pads drive players 2 to 4. Only changes are sent. ACT-EMU-002 (co-op)
// feeds remote players through the same `send`.
import {
  assignPlayers,
  BUTTONS,
  diffButtons,
  MAX_PLAYERS,
  readKeyboard,
  readPad,
  type Button,
  type PadLike,
  type PadMapping,
} from './gamepad';

export type SendButton = (player: number, button: Button, pressed: boolean) => void;

export class InputPoller {
  private readonly held = new Set<string>();
  private prev: Array<Set<Button>> = Array.from({ length: MAX_PLAYERS }, () => new Set<Button>());

  constructor(
    private readonly send: SendButton,
    private readonly mapping: () => PadMapping,
    private readonly pads: () => ReadonlyArray<PadLike | null>,
  ) {}

  /** A key went down or up (`KeyboardEvent.code`). */
  key(code: string, down: boolean): void {
    if (down) this.held.add(code);
    else this.held.delete(code);
  }

  /** The buttons each player holds right now. */
  state(): Array<Set<Button>> {
    const map = this.mapping();
    const out = Array.from({ length: MAX_PLAYERS }, () => new Set<Button>());
    for (const b of readKeyboard(this.held, map)) out[0]?.add(b);
    assignPlayers(this.pads()).forEach((pad, i) => {
      for (const b of readPad(pad, map)) out[i]?.add(b);
    });
    return out;
  }

  /** Read the devices and send what changed. */
  poll(): void {
    const now = this.state();
    now.forEach((next, player) => {
      const before = this.prev[player] ?? new Set<Button>();
      for (const ev of diffButtons(player, before, next))
        this.send(ev.player, ev.button, ev.pressed);
      this.prev[player] = next;
    });
  }

  /** Let go of everything (focus lost, pause, close). */
  releaseAll(): void {
    this.held.clear();
    this.prev.forEach((was, player) => {
      for (const b of BUTTONS) if (was.has(b)) this.send(player, b, false);
      this.prev[player] = new Set<Button>();
    });
  }
}
