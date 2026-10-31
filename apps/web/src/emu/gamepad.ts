// ACT-EMU-001: input for the emulator. Pure mapping logic (no DOM): keyboard codes and Gamepad API state in, the
// pressed RetroPad buttons out, plus the capture state machine of the mapping screen and the saved mapping.
//
// The button order is RetroArch's joypad order: the core's `simulate_input(player, index, value)` takes the index.

export const BUTTONS = [
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
] as const;
export type Button = (typeof BUTTONS)[number];

/** RetroArch's joypad index of a button. */
export const retroIndex = (b: Button): number => BUTTONS.indexOf(b);

/** A gamepad control: a button by index, or one direction of an axis. */
export type PadBinding =
  { kind: 'button'; index: number } | { kind: 'axis'; index: number; dir: -1 | 1 };

export interface PadMapping {
  /** `KeyboardEvent.code` per button. */
  keyboard: Partial<Record<Button, string>>;
  /** The gamepad control per button. */
  gamepad: Partial<Record<Button, PadBinding>>;
}

/** Axis value past which a stick counts as pressed. */
export const AXIS_THRESHOLD = 0.5;
export const MAX_PLAYERS = 4;

export const DEFAULT_KEYBOARD: Readonly<Partial<Record<Button, string>>> = {
  up: 'ArrowUp',
  down: 'ArrowDown',
  left: 'ArrowLeft',
  right: 'ArrowRight',
  b: 'KeyZ',
  a: 'KeyX',
  y: 'KeyA',
  x: 'KeyS',
  l: 'KeyQ',
  r: 'KeyW',
  start: 'Enter',
  select: 'ShiftRight',
};

/** The browser's "standard" gamepad layout onto RetroPad: south is B, east is A, west is Y, north is X. */
export const DEFAULT_GAMEPAD: Readonly<Partial<Record<Button, PadBinding>>> = {
  b: { kind: 'button', index: 0 },
  a: { kind: 'button', index: 1 },
  y: { kind: 'button', index: 2 },
  x: { kind: 'button', index: 3 },
  l: { kind: 'button', index: 4 },
  r: { kind: 'button', index: 5 },
  l2: { kind: 'button', index: 6 },
  r2: { kind: 'button', index: 7 },
  select: { kind: 'button', index: 8 },
  start: { kind: 'button', index: 9 },
  l3: { kind: 'button', index: 10 },
  r3: { kind: 'button', index: 11 },
  up: { kind: 'button', index: 12 },
  down: { kind: 'button', index: 13 },
  left: { kind: 'button', index: 14 },
  right: { kind: 'button', index: 15 },
};

export const defaultMapping = (): PadMapping => ({
  keyboard: { ...DEFAULT_KEYBOARD },
  gamepad: { ...DEFAULT_GAMEPAD },
});

/** The part of the Gamepad API the mapping reads (a real `Gamepad` satisfies it). */
export interface PadLike {
  id: string;
  index: number;
  connected: boolean;
  buttons: ReadonlyArray<{ pressed: boolean; value: number }>;
  axes: ReadonlyArray<number>;
}

const bindingDown = (b: PadBinding, pad: PadLike): boolean => {
  if (b.kind === 'button') {
    const btn = pad.buttons[b.index];
    return !!btn && (btn.pressed || btn.value > AXIS_THRESHOLD);
  }
  const v = pad.axes[b.index] ?? 0;
  return b.dir < 0 ? v < -AXIS_THRESHOLD : v > AXIS_THRESHOLD;
};

/** The buttons a gamepad holds down under a mapping. A left stick also drives the d-pad when the d-pad itself is idle. */
export function readPad(pad: PadLike, map: PadMapping): Set<Button> {
  const down = new Set<Button>();
  for (const b of BUTTONS) {
    const bind = map.gamepad[b];
    if (bind && bindingDown(bind, pad)) down.add(b);
  }
  const dpad = ['up', 'down', 'left', 'right'].some((d) => down.has(d as Button));
  if (!dpad) {
    const x = pad.axes[0] ?? 0;
    const y = pad.axes[1] ?? 0;
    if (x < -AXIS_THRESHOLD) down.add('left');
    if (x > AXIS_THRESHOLD) down.add('right');
    if (y < -AXIS_THRESHOLD) down.add('up');
    if (y > AXIS_THRESHOLD) down.add('down');
  }
  return down;
}

/** The buttons the keyboard holds down: `held` is the set of `KeyboardEvent.code`s currently pressed. */
export function readKeyboard(held: ReadonlySet<string>, map: PadMapping): Set<Button> {
  const down = new Set<Button>();
  for (const b of BUTTONS) {
    const code = map.keyboard[b];
    if (code && held.has(code)) down.add(b);
  }
  return down;
}

export interface ButtonEvent {
  player: number;
  button: Button;
  pressed: boolean;
}

/** What changed between two polls of the same player. */
export function diffButtons(player: number, prev: ReadonlySet<Button>, next: ReadonlySet<Button>) {
  const out: ButtonEvent[] = [];
  for (const b of BUTTONS) {
    const was = prev.has(b);
    const is = next.has(b);
    if (was !== is) out.push({ player, button: b, pressed: is });
  }
  return out;
}

/** The connected pads in port order, at most [MAX_PLAYERS]; the first connected pad is player 1. */
export function assignPlayers(pads: ReadonlyArray<PadLike | null>): PadLike[] {
  return pads
    .filter((p): p is PadLike => !!p && p.connected)
    .sort((a, b) => a.index - b.index)
    .slice(0, MAX_PLAYERS);
}

// ---------------------------------------------------------------- the mapping screen

export type CaptureTarget = 'keyboard' | 'gamepad';
/** Order the mapping screen asks for. */
export const CAPTURE_ORDER: readonly Button[] = [
  'up',
  'down',
  'left',
  'right',
  'a',
  'b',
  'x',
  'y',
  'l',
  'r',
  'start',
  'select',
];

export interface CaptureState {
  target: CaptureTarget;
  /** The button waiting for an input; `null` when done. */
  current: Button | null;
  mapping: PadMapping;
}

export const startCapture = (target: CaptureTarget, from: PadMapping): CaptureState => ({
  target,
  current: CAPTURE_ORDER[0] ?? null,
  mapping: { keyboard: { ...from.keyboard }, gamepad: { ...from.gamepad } },
});

export type CaptureInput = { key: string } | { binding: PadBinding };

const sameBinding = (a: PadBinding | undefined, b: PadBinding) =>
  !!a &&
  a.kind === b.kind &&
  a.index === b.index &&
  (a.kind === 'button' || (b.kind === 'axis' && a.dir === b.dir));

/** Take the next input for the button being asked for, and move on. A control already used elsewhere is taken from it. */
export function captureStep(state: CaptureState, input: CaptureInput): CaptureState {
  const cur = state.current;
  if (!cur) return state;
  const mapping: PadMapping = {
    keyboard: { ...state.mapping.keyboard },
    gamepad: { ...state.mapping.gamepad },
  };
  if (state.target === 'keyboard' && 'key' in input) {
    for (const b of BUTTONS) if (mapping.keyboard[b] === input.key) delete mapping.keyboard[b];
    mapping.keyboard[cur] = input.key;
  } else if (state.target === 'gamepad' && 'binding' in input) {
    for (const b of BUTTONS)
      if (sameBinding(mapping.gamepad[b], input.binding)) delete mapping.gamepad[b];
    mapping.gamepad[cur] = input.binding;
  } else {
    return state;
  }
  const at = CAPTURE_ORDER.indexOf(cur);
  return { ...state, mapping, current: CAPTURE_ORDER[at + 1] ?? null };
}

/** Skip the button being asked for (leave it as it was). */
export function captureSkip(state: CaptureState): CaptureState {
  if (!state.current) return state;
  const at = CAPTURE_ORDER.indexOf(state.current);
  return { ...state, current: CAPTURE_ORDER[at + 1] ?? null };
}

/** The first pad control that is held right now: what a "press a button" prompt listens for. */
export function firstBinding(pad: PadLike): PadBinding | null {
  for (let i = 0; i < pad.buttons.length; i++) {
    const b = pad.buttons[i];
    if (b && (b.pressed || b.value > 0.8)) return { kind: 'button', index: i };
  }
  for (let i = 0; i < pad.axes.length; i++) {
    const v = pad.axes[i] ?? 0;
    if (v < -0.8) return { kind: 'axis', index: i, dir: -1 };
    if (v > 0.8) return { kind: 'axis', index: i, dir: 1 };
  }
  return null;
}

// ---------------------------------------------------------------- persistence

export const MAPPING_KEY = 'sr.emu.pad.v1';

const isBinding = (v: unknown): v is PadBinding => {
  if (!v || typeof v !== 'object') return false;
  const o = v as Record<string, unknown>;
  if (typeof o.index !== 'number' || !Number.isInteger(o.index) || o.index < 0 || o.index > 64)
    return false;
  if (o.kind === 'button') return true;
  return o.kind === 'axis' && (o.dir === 1 || o.dir === -1);
};

/** Read a saved mapping, ignoring anything malformed (a bad entry falls back to the default for that button). */
export function parseMapping(raw: string | null): PadMapping {
  const out = defaultMapping();
  if (!raw) return out;
  try {
    const j = JSON.parse(raw) as {
      keyboard?: Record<string, unknown>;
      gamepad?: Record<string, unknown>;
    };
    for (const b of BUTTONS) {
      const k = j.keyboard?.[b];
      if (typeof k === 'string' && k.length > 0 && k.length < 32) out.keyboard[b] = k;
      const g = j.gamepad?.[b];
      if (isBinding(g)) out.gamepad[b] = g;
    }
  } catch {
    /* unreadable: the defaults stay */
  }
  return out;
}

export function loadMapping(store: Pick<Storage, 'getItem'> | null): PadMapping {
  try {
    return parseMapping(store?.getItem(MAPPING_KEY) ?? null);
  } catch {
    return defaultMapping();
  }
}

export function saveMapping(store: Pick<Storage, 'setItem'> | null, map: PadMapping): void {
  try {
    store?.setItem(MAPPING_KEY, JSON.stringify(map));
  } catch {
    /* storage blocked: the mapping lasts for this visit only */
  }
}
