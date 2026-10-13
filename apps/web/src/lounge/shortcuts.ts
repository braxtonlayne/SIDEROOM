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
