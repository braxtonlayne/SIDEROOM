// Human-readable audit log text (LNG-SSET-057). Action numbers are Discord's (sr-chat audit.rs).
import { t, type MessageKey } from '../../i18n';
import type { AuditChange, AuditEntry } from './api';
import { permissionNames, toBig } from './permissions';

export type TargetKind = 'user' | 'role' | 'channel' | 'other';

interface ActionInfo {
  /** Past tense phrase, followed by the target name when there is one. */
  text: string;
  target: TargetKind;
}

/** `text` resolves when read, so it follows the active language. */
const A = (key: MessageKey, target: TargetKind = 'other'): ActionInfo => ({
  get text() {
    return t(key);
  },
  target,
});

export const AUDIT_ACTIONS: Record<number, ActionInfo> = {
  1: A('settings.auditText.a1'),
  10: A('settings.auditText.a10', 'channel'),
  11: A('settings.auditText.a11', 'channel'),
  12: A('settings.auditText.a12', 'channel'),
  13: A('settings.auditText.a13', 'channel'),
  14: A('settings.auditText.a14', 'channel'),
  15: A('settings.auditText.a15', 'channel'),
  20: A('settings.auditText.a20', 'user'),
  21: A('settings.auditText.a21'),
  22: A('settings.auditText.a22', 'user'),
  23: A('settings.auditText.a23', 'user'),
  24: A('settings.auditText.a24', 'user'),
  25: A('settings.auditText.a25', 'user'),
  26: A('settings.auditText.a26'),
  27: A('settings.auditText.a27'),
  28: A('settings.auditText.a28', 'user'),
  30: A('settings.auditText.a30', 'role'),
  31: A('settings.auditText.a31', 'role'),
  32: A('settings.auditText.a32', 'role'),
  40: A('settings.auditText.a40'),
  41: A('settings.auditText.a41'),
  42: A('settings.auditText.a42'),
  50: A('settings.auditText.a50'),
  51: A('settings.auditText.a51'),
  52: A('settings.auditText.a52'),
  60: A('settings.auditText.a60'),
  61: A('settings.auditText.a61'),
  62: A('settings.auditText.a62'),
  72: A('settings.auditText.a72', 'user'),
  73: A('settings.auditText.a73'),
  74: A('settings.auditText.a74'),
  75: A('settings.auditText.a75'),
  80: A('settings.auditText.a80'),
  81: A('settings.auditText.a81'),
  82: A('settings.auditText.a82'),
  90: A('settings.auditText.a90'),
  91: A('settings.auditText.a91'),
  92: A('settings.auditText.a92'),
  100: A('settings.auditText.a100'),
  101: A('settings.auditText.a101'),
  102: A('settings.auditText.a102'),
  110: A('settings.auditText.a110'),
  111: A('settings.auditText.a111'),
  112: A('settings.auditText.a112'),
  130: A('settings.auditText.a130'),
  131: A('settings.auditText.a131'),
  132: A('settings.auditText.a132'),
  140: A('settings.auditText.a140'),
  141: A('settings.auditText.a141'),
  142: A('settings.auditText.a142'),
  143: A('settings.auditText.a143', 'user'),
  144: A('settings.auditText.a144', 'user'),
  145: A('settings.auditText.a145', 'user'),
  163: A('settings.auditText.a163'),
  164: A('settings.auditText.a164'),
  165: A('settings.auditText.a165'),
  166: A('settings.auditText.a166'),
  167: A('settings.auditText.a167'),
  200: A('settings.auditText.a200', 'user'),
  210: A('settings.auditText.a210'),
  211: A('settings.auditText.a211'),
  212: A('settings.auditText.a212'),
  213: A('settings.auditText.a213'),
};

/** The action filter options, in number order. */
export const AUDIT_FILTERS: { value: number; label: string }[] = Object.entries(AUDIT_ACTIONS).map(
  ([code, info]) => ({
    value: Number(code),
    get label() {
      return sentence(info.text);
    },
  }),
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

/** Built-in module writes carry `options.module`; the actor then reads "SIDE ROOM · <module>" (D-187). */
export function actorLabel(entry: AuditEntry, fallback: string): string {
  const m = entry.options?.module;
  return typeof m === 'string' && m ? t('settings.auditText.moduleActor', { module: m }) : fallback;
}

/** "Alice banned Bob" / "Alice created an invite". */
export function describeEntry(entry: AuditEntry, actor: string, names: Names): string {
  const info = AUDIT_ACTIONS[entry.action_type];
  if (!info) return t('settings.auditText.performed', { actor, action: entry.action_type });
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
  if (v === null || v === undefined) return t('settings.auditText.none');
  if (PERMISSION_KEYS.has(key) && (typeof v === 'string' || typeof v === 'number')) {
    const names = permissionNames(toBig(v));
    return names.length ? names.join(', ') : t('settings.auditText.noPermissions');
  }
  if (key === 'color' && typeof v === 'number')
    return v ? `#${v.toString(16).padStart(6, '0')}` : t('settings.auditText.default');
  if (typeof v === 'boolean') return v ? t('settings.auditText.on') : t('settings.auditText.off');
  if (Array.isArray(v))
    return v.length
      ? v.map((x) => (typeof x === 'object' ? JSON.stringify(x) : String(x))).join(', ')
      : t('settings.auditText.none');
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
