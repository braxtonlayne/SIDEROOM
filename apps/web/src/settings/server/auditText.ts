// Human-readable audit log text (LNG-SSET-057). Action numbers are Discord's (sr-chat audit.rs).
import type { AuditChange, AuditEntry } from './api';
import { permissionNames, toBig } from './permissions';

export type TargetKind = 'user' | 'role' | 'channel' | 'other';

interface ActionInfo {
  /** Past tense phrase, followed by the target name when there is one. */
  text: string;
  target: TargetKind;
}

const A = (text: string, target: TargetKind = 'other'): ActionInfo => ({ text, target });

export const AUDIT_ACTIONS: Record<number, ActionInfo> = {
  1: A('updated the server settings'),
  10: A('created channel', 'channel'),
  11: A('updated channel', 'channel'),
  12: A('deleted channel', 'channel'),
  13: A('created a permission overwrite on', 'channel'),
  14: A('updated a permission overwrite on', 'channel'),
  15: A('deleted a permission overwrite on', 'channel'),
  20: A('kicked', 'user'),
  21: A('pruned members'),
  22: A('banned', 'user'),
  23: A('unbanned', 'user'),
  24: A('updated member', 'user'),
  25: A('updated the roles of', 'user'),
  26: A('moved a member between voice channels'),
  27: A('disconnected a member from voice'),
  28: A('added the bot', 'user'),
  30: A('created role', 'role'),
  31: A('updated role', 'role'),
  32: A('deleted role', 'role'),
  40: A('created an invite'),
  41: A('updated an invite'),
  42: A('deleted an invite'),
  50: A('created a webhook'),
  51: A('updated a webhook'),
  52: A('deleted a webhook'),
  60: A('created an emoji'),
  61: A('updated an emoji'),
  62: A('deleted an emoji'),
  72: A('deleted a message by', 'user'),
  73: A('bulk deleted messages'),
  74: A('pinned a message'),
  75: A('unpinned a message'),
  80: A('added an integration'),
  81: A('updated an integration'),
  82: A('removed an integration'),
  90: A('created a sticker'),
  91: A('updated a sticker'),
  92: A('deleted a sticker'),
  100: A('created an event'),
  101: A('updated an event'),
  102: A('deleted an event'),
  110: A('created a thread'),
  111: A('updated a thread'),
  112: A('deleted a thread'),
  130: A('added a soundboard sound'),
  131: A('updated a soundboard sound'),
  132: A('removed a soundboard sound'),
  140: A('created an AutoMod rule'),
  141: A('updated an AutoMod rule'),
  142: A('deleted an AutoMod rule'),
  143: A('had AutoMod block a message from', 'user'),
  144: A('had AutoMod flag a message from', 'user'),
  145: A('had AutoMod time out', 'user'),
  163: A('created an onboarding prompt'),
  164: A('updated an onboarding prompt'),
  165: A('deleted an onboarding prompt'),
  166: A('set up onboarding'),
  167: A('updated onboarding'),
};

/** The action filter options, in number order. */
export const AUDIT_FILTERS: { value: number; label: string }[] = Object.entries(AUDIT_ACTIONS).map(
  ([code, info]) => ({ value: Number(code), label: sentence(info.text) }),
);

function sentence(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function targetKind(action: number): TargetKind {
  return AUDIT_ACTIONS[action]?.target ?? 'other';
}

export interface Names {
  user: (id: string) => string;
  role: (id: string) => string;
  channel: (id: string) => string;
}

/** "Alice banned Bob" / "Alice created an invite". */
export function describeEntry(entry: AuditEntry, actor: string, names: Names): string {
  const info = AUDIT_ACTIONS[entry.action_type];
  if (!info) return `${actor} performed action ${entry.action_type}`;
  let target = '';
  if (entry.target_id && info.target !== 'other') {
    const n = names[info.target](entry.target_id);
    target = info.target === 'channel' ? ` ${n.startsWith('#') ? n : `#${n}`}` : ` ${n}`;
  }
  return `${actor} ${info.text}${target}`;
}

const PERMISSION_KEYS = new Set(['permissions', 'allow', 'deny', 'permissions_new']);

const prettyKey = (k: string) => k.replace(/_/g, ' ');

function show(key: string, v: unknown): string {
  if (v === null || v === undefined) return 'none';
  if (PERMISSION_KEYS.has(key) && (typeof v === 'string' || typeof v === 'number')) {
    const names = permissionNames(toBig(v));
    return names.length ? names.join(', ') : 'no permissions';
  }
  if (key === 'color' && typeof v === 'number')
    return v ? `#${v.toString(16).padStart(6, '0')}` : 'default';
  if (typeof v === 'boolean') return v ? 'on' : 'off';
  if (Array.isArray(v))
    return v.length
      ? v.map((x) => (typeof x === 'object' ? JSON.stringify(x) : String(x))).join(', ')
      : 'none';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

export interface DiffLine {
  key: string;
  label: string;
  before: string | null;
  after: string | null;
}

/** One line per changed key; created/deleted objects only have one side. */
export function diffLines(changes: AuditChange[]): DiffLine[] {
  return changes.map((c) => ({
    key: c.key,
    label: prettyKey(c.key),
    before: c.old_value === undefined ? null : show(c.key, c.old_value),
    after: c.new_value === undefined ? null : show(c.key, c.new_value),
  }));
}
