/** Keyboard shortcut model for voice keybinds (PTT, mute, deafen): matching, display, recording. */
export interface Keybind {
  /** `KeyboardEvent.code` of the main key (layout independent), e.g. "KeyV", "ControlLeft". */
  code: string;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
  meta?: boolean;
}

const MODIFIER_CODES = /^(Control|Alt|Shift|Meta|OS)(Left|Right)$/;
export const isModifierCode = (code: string) => MODIFIER_CODES.test(code);

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** Does this key event press the bind (main key plus every required modifier)? */
export function matchesKeydown(b: Keybind, e: KeyLike): boolean {
  if (e.code !== b.code) return false;
  if (isModifierCode(b.code)) return true;
  return (
    !!b.ctrl === e.ctrlKey &&
    !!b.alt === e.altKey &&
    !!b.shift === e.shiftKey &&
    !!b.meta === e.metaKey
  );
}

/** Does releasing this key end the bind (its main key, or a modifier it needs)? */
export function endsOnKeyup(b: Keybind, e: { code: string }): boolean {
  if (e.code === b.code) return true;
  if (/^Control/.test(e.code)) return !!b.ctrl;
  if (/^Alt/.test(e.code)) return !!b.alt;
  if (/^Shift/.test(e.code)) return !!b.shift;
  if (/^(Meta|OS)/.test(e.code)) return !!b.meta;
  return false;
}

export const sameKeybind = (a: Keybind | null | undefined, b: Keybind | null | undefined) =>
  !!a &&
  !!b &&
  a.code === b.code &&
  !!a.ctrl === !!b.ctrl &&
  !!a.alt === !!b.alt &&
  !!a.shift === !!b.shift &&
  !!a.meta === !!b.meta;

/** Build a bind from a keydown event, or null for keys that cannot be bound (Escape cancels recording). */
export function keybindFromEvent(e: KeyLike): Keybind | null {
  if (!e.code || e.code === 'Escape' || e.code === 'Unidentified') return null;
  if (isModifierCode(e.code)) return { code: e.code };
  return {
    code: e.code,
    ...(e.ctrlKey ? { ctrl: true } : {}),
    ...(e.altKey ? { alt: true } : {}),
    ...(e.shiftKey ? { shift: true } : {}),
    ...(e.metaKey ? { meta: true } : {}),
  };
}

const NAMES: Record<string, string> = {
  Space: 'Space',
  Backquote: '`',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  ControlLeft: 'Left Ctrl',
  ControlRight: 'Right Ctrl',
  AltLeft: 'Left Alt',
  AltRight: 'Right Alt',
  ShiftLeft: 'Left Shift',
  ShiftRight: 'Right Shift',
  MetaLeft: 'Left Meta',
  MetaRight: 'Right Meta',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  CapsLock: 'Caps Lock',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
};

export function codeLabel(code: string): string {
  const named = NAMES[code];
  if (named) return named;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad/.test(code)) return `Numpad ${code.slice(6)}`;
  return code.replace(/([a-z])([A-Z])/g, '$1 $2');
}

/** Keys to show in a chord, e.g. ["Ctrl", "Shift", "V"]. */
export function keybindParts(b: Keybind): string[] {
  const mods: string[] = [];
  if (!isModifierCode(b.code)) {
    if (b.ctrl) mods.push('Ctrl');
    if (b.alt) mods.push('Alt');
    if (b.shift) mods.push('Shift');
    if (b.meta) mods.push('Meta');
  }
  return [...mods, codeLabel(b.code)];
}

export const keybindText = (b: Keybind | null | undefined) =>
  b ? keybindParts(b).join(' + ') : 'Not bound';

export function sanitizeKeybind(v: unknown): Keybind | null {
  if (typeof v !== 'object' || v === null) return null;
  const r = v as Record<string, unknown>;
  if (typeof r.code !== 'string' || !r.code || r.code.length > 40) return null;
  return {
    code: r.code,
    ...(r.ctrl === true ? { ctrl: true } : {}),
    ...(r.alt === true ? { alt: true } : {}),
    ...(r.shift === true ? { shift: true } : {}),
    ...(r.meta === true ? { meta: true } : {}),
  };
}
