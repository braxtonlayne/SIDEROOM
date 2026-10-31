/**
 * ACT-EMU-002: pad input. The pad is the libretro RetroPad, the layout the cores (and EmulatorJS's
 * `simulate_input(player, button, value)`) use: button n is bit n of a 16-bit mask. Keyboards and gamepads map
 * onto it here; the host applies a received mask to the core's port for that player.
 */

export const PAD = {
  B: 0,
  Y: 1,
  SELECT: 2,
  START: 3,
  UP: 4,
  DOWN: 5,
  LEFT: 6,
  RIGHT: 7,
  A: 8,
  X: 9,
  L: 10,
  R: 11,
  L2: 12,
  R2: 13,
  L3: 14,
  R3: 15,
} as const;

export const bit = (button: number): number => 1 << button;

/** Default keyboard layout of a guest, by `KeyboardEvent.code`. */
export const DEFAULT_KEYS: Readonly<Record<string, number>> = {
  ArrowUp: PAD.UP,
  ArrowDown: PAD.DOWN,
  ArrowLeft: PAD.LEFT,
  ArrowRight: PAD.RIGHT,
  KeyZ: PAD.B,
  KeyX: PAD.A,
  KeyA: PAD.Y,
  KeyS: PAD.X,
  KeyQ: PAD.L,
  KeyW: PAD.R,
  Enter: PAD.START,
  ShiftRight: PAD.SELECT,
  Backspace: PAD.SELECT,
};

/**
 * The "standard" Gamepad API layout (w3c.github.io/gamepad/#remapping) onto RetroPad: the bottom face button
 * is B, the right one A, left Y, top X, as on a SNES pad.
 */
const STANDARD_BUTTONS: readonly number[] = [
  PAD.B, // 0 bottom
  PAD.A, // 1 right
  PAD.Y, // 2 left
  PAD.X, // 3 top
  PAD.L, // 4
  PAD.R, // 5
  PAD.L2, // 6
  PAD.R2, // 7
  PAD.SELECT, // 8
  PAD.START, // 9
  PAD.L3, // 10
  PAD.R3, // 11
  PAD.UP, // 12
  PAD.DOWN, // 13
  PAD.LEFT, // 14
  PAD.RIGHT, // 15
];

export const STICK_DEADZONE = 0.5;

export interface GamepadLike {
  buttons: ReadonlyArray<{ pressed: boolean }>;
  axes: ReadonlyArray<number>;
}

export function cancelOpposites(mask: number): number {
  let m = mask;
  if (m & bit(PAD.UP) && m & bit(PAD.DOWN)) m &= ~(bit(PAD.UP) | bit(PAD.DOWN));
  if (m & bit(PAD.LEFT) && m & bit(PAD.RIGHT)) m &= ~(bit(PAD.LEFT) | bit(PAD.RIGHT));
  return m;
}

/** The mask of one gamepad: its buttons plus the left stick as the d-pad. */
export function maskFromGamepad(pad: GamepadLike): number {
  let mask = 0;
  STANDARD_BUTTONS.forEach((retro, i) => {
    if (pad.buttons[i]?.pressed) mask |= bit(retro);
  });
  const x = pad.axes[0] ?? 0;
  const y = pad.axes[1] ?? 0;
  if (x <= -STICK_DEADZONE) mask |= bit(PAD.LEFT);
  if (x >= STICK_DEADZONE) mask |= bit(PAD.RIGHT);
  if (y <= -STICK_DEADZONE) mask |= bit(PAD.UP);
  if (y >= STICK_DEADZONE) mask |= bit(PAD.DOWN);
  return cancelOpposites(mask);
}

/** Keeps the held keys and gives the mask; opposite directions cancel (a core cannot take up+down). */
export class KeyboardPad {
  private readonly held = new Set<number>();
  constructor(private readonly layout: Readonly<Record<string, number>> = DEFAULT_KEYS) {}

  /** True when the key belongs to the pad (so the page should not also act on it). */
  press(code: string): boolean {
    const b = this.layout[code];
    if (b === undefined) return false;
    this.held.add(b);
    return true;
  }
  release(code: string): boolean {
    const b = this.layout[code];
    if (b === undefined) return false;
    this.held.delete(b);
    return true;
  }
  clear(): void {
    this.held.clear();
  }
  get mask(): number {
    let m = 0;
    for (const b of this.held) m |= bit(b);
    return cancelOpposites(m);
  }
}

/** The buttons whose state differs between two masks, as [button, pressed] pairs in button order. */
export function diffMasks(prev: number, next: number): Array<[button: number, pressed: boolean]> {
  const changed = (prev ^ next) & 0xffff;
  const out: Array<[number, boolean]> = [];
  for (let b = 0; b < 16; b += 1) {
    if (changed & bit(b)) out.push([b, (next & bit(b)) !== 0]);
  }
  return out;
}
