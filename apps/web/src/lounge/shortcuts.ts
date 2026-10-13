/** Single source of truth for Lounge keyboard shortcuts; the shortcut sheet (Ctrl+/) renders this list. */

export type ShortcutId =
  | 'quickSwitcher'
  | 'shortcutSheet'
  | 'userSettings'
  | 'prevChannel'
  | 'nextChannel'
  | 'prevUnread'
  | 'nextUnread'
  | 'prevGuild'
  | 'nextGuild'
  | 'markRead'
  | 'focusComposer';

export interface ShortcutDef {
  id: ShortcutId;
  group: 'Navigation' | 'Messages';
  description: string;
  /** Display keys; "Mod" renders as Ctrl (⌘ on Apple platforms). */
  keys: string[];
}

export const SHORTCUTS: readonly ShortcutDef[] = [
  {
    id: 'quickSwitcher',
    group: 'Navigation',
    description: 'Find or start a conversation',
    keys: ['Mod', 'K'],
  },
  {
    id: 'shortcutSheet',
    group: 'Navigation',
    description: 'Show keyboard shortcuts',
    keys: ['Mod', '/'],
  },
  {
    id: 'userSettings',
    group: 'Navigation',
    description: 'Open user settings',
    keys: ['Mod', ','],
  },
  { id: 'prevChannel', group: 'Navigation', description: 'Previous channel', keys: ['Alt', '↑'] },
  { id: 'nextChannel', group: 'Navigation', description: 'Next channel', keys: ['Alt', '↓'] },
  {
    id: 'prevUnread',
    group: 'Navigation',
    description: 'Previous unread channel',
    keys: ['Alt', 'Shift', '↑'],
  },
  {
    id: 'nextUnread',
    group: 'Navigation',
    description: 'Next unread channel',
    keys: ['Alt', 'Shift', '↓'],
  },
  {
    id: 'prevGuild',
    group: 'Navigation',
    description: 'Previous server',
    keys: ['Mod', 'Alt', '↑'],
  },
  { id: 'nextGuild', group: 'Navigation', description: 'Next server', keys: ['Mod', 'Alt', '↓'] },
  { id: 'markRead', group: 'Messages', description: 'Mark channel as read', keys: ['Esc'] },
  {
    // Discord behaviour: any printable key outside a field moves focus to the message box (Q169).
    id: 'focusComposer',
    group: 'Messages',
    description: 'Start typing a message (focuses the message box)',
    keys: ['Any key'],
  },
];

export function matchShortcut(
  e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>,
): ShortcutId | null {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (mod && !e.altKey && !e.shiftKey && k === 'k') return 'quickSwitcher';
  if (mod && !e.altKey && !e.shiftKey && k === '/') return 'shortcutSheet';
  if (mod && !e.altKey && !e.shiftKey && k === ',') return 'userSettings';
  if (k === 'ArrowUp' || k === 'ArrowDown') {
    const up = k === 'ArrowUp';
    if (mod && e.altKey && !e.shiftKey) return up ? 'prevGuild' : 'nextGuild';
    if (!mod && e.altKey && e.shiftKey) return up ? 'prevUnread' : 'nextUnread';
    if (!mod && e.altKey && !e.shiftKey) return up ? 'prevChannel' : 'nextChannel';
  }
  if (k === 'Escape' && !mod && !e.altKey && !e.shiftKey) return 'markRead';
  // A printable character with no Ctrl/Meta/Alt (Shift allowed) starts a message.
  if (!mod && !e.altKey && e.key.length === 1) return 'focusComposer';
  return null;
}

export const isApple = () =>
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
export const displayKey = (k: string) => (k === 'Mod' ? (isApple() ? '⌘' : 'Ctrl') : k);

// ---- custom keybinds (User Settings > Keybinds) ----

/** Contextual keys that stay where they are: Esc depends on dialogs, "any key" is not a combo. */
const FIXED: ReadonlySet<ShortcutId> = new Set<ShortcutId>(['markRead', 'focusComposer']);
export const isRebindable = (id: ShortcutId) => !FIXED.has(id);

const ARROW: Record<string, string> = { '↑': 'ArrowUp', '↓': 'ArrowDown' };
const ARROW_SHOWN: Record<string, string> = {
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
};

/** "Mod+Alt+ArrowUp": modifiers in the order Mod, Alt, Shift, then the key. */
export const defaultCombo = (def: ShortcutDef): string =>
  def.keys.map((k) => ARROW[k] ?? (k.length === 1 ? k.toUpperCase() : k)).join('+');

/** The combo a key event stands for, or null for a bare modifier press. */
export function comboFromEvent(
  e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>,
): string | null {
  if (['Control', 'Meta', 'Alt', 'Shift', 'AltGraph', 'Dead', 'Unidentified'].includes(e.key))
    return null;
  const key = e.key === ' ' ? 'Space' : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push('Mod');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  parts.push(key);
  return parts.join('+');
}

/** Keys of a combo for display ("Mod+Alt+ArrowUp" becomes Ctrl, Alt, ↑). */
export function comboKeys(combo: string): string[] {
  if (combo === '') return [];
  const m = /^((?:(?:Mod|Alt|Shift)\+)*)(.+)$/.exec(combo);
  if (!m) return [combo];
  const mods = (m[1] ?? '').split('+').filter(Boolean);
  const key = m[2] ?? '';
  return [...mods, ARROW_SHOWN[key] ?? key].map(displayKey);
}

/** The combo in force for a shortcut: the user's choice ('' = unbound) or the default. */
export const effectiveCombo = (def: ShortcutDef, overrides: Record<string, string>): string =>
  overrides[def.id] ?? defaultCombo(def);

/** Which shortcut a key event triggers once the user's keybinds are applied. */
export function matchCustomShortcut(
  e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>,
  overrides: Record<string, string>,
): ShortcutId | null {
  const fixed = matchShortcut(e);
  if (fixed && FIXED.has(fixed)) return fixed;
  const combo = comboFromEvent(e);
  if (!combo) return null;
  for (const def of SHORTCUTS)
    if (isRebindable(def.id) && effectiveCombo(def, overrides) === combo) return def.id;
  return null;
}
