// Permission catalog for the server settings screens (docs/07 section 2, sr-core permissions.rs).
// Bit numbers match Discord for 0-52; SIDE ROOM adds 56-60. Bitfields travel as decimal strings.
import { t } from '../../i18n';
import type { PermissionGroupDef, TriState } from '@sideroom/ui';

export interface PermissionInfo {
  /** Constant name, also the id used by the PermissionMatrix. */
  id: string;
  bit: number;
  label: string;
  description: string;
  group: string;
  /** Meaningful on text channels only / voice channels only (overwrite editor filters on this). */
  scope?: 'text' | 'voice' | 'guild';
  danger?: boolean;
}

export const GROUPS = [
  {
    id: 'general',
    get label() {
      return t('settings.permissions.groupGeneral');
    },
  },
  {
    id: 'membership',
    get label() {
      return t('settings.permissions.groupMembership');
    },
  },
  {
    id: 'text',
    get label() {
      return t('settings.permissions.groupText');
    },
  },
  {
    id: 'voice',
    get label() {
      return t('settings.permissions.groupVoice');
    },
  },
  {
    id: 'apps',
    get label() {
      return t('settings.permissions.groupApps');
    },
  },
  {
    id: 'events',
    get label() {
      return t('settings.permissions.groupEvents');
    },
  },
  {
    id: 'advanced',
    get label() {
      return t('settings.permissions.groupAdvanced');
    },
  },
  {
    id: 'sideroom',
    get label() {
      return t('settings.permissions.groupSideroom');
    },
  },
] as const;

/** Label and description live in the catalog (`settings.permissions.<ID>.label|description`) and resolve when read. */
const p = (
  id: string,
  bit: number,
  group: string,
  extra: Partial<PermissionInfo> = {},
): PermissionInfo => {
  const key = id.toLowerCase().replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
  return {
    id,
    bit,
    group,
    get label() {
      return t(`settings.permissions.${key}.label`);
    },
    get description() {
      return t(`settings.permissions.${key}.description`);
    },
    ...extra,
  };
};

export const PERMISSIONS: PermissionInfo[] = [
  p('VIEW_CHANNEL', 10, 'general'),
  p('MANAGE_CHANNELS', 4, 'general', { scope: 'guild' }),
  p('MANAGE_ROLES', 28, 'general'),
  p('MANAGE_GUILD_EXPRESSIONS', 30, 'general', { scope: 'guild' }),
  p('CREATE_GUILD_EXPRESSIONS', 43, 'general', { scope: 'guild' }),
  p('VIEW_AUDIT_LOG', 7, 'general', {
    scope: 'guild',
  }),
  p('VIEW_GUILD_INSIGHTS', 19, 'general', { scope: 'guild' }),
  p('MANAGE_WEBHOOKS', 29, 'general'),
  p('MANAGE_GUILD', 5, 'general', { scope: 'guild' }),

  p('CREATE_INSTANT_INVITE', 0, 'membership'),
  p('CHANGE_NICKNAME', 26, 'membership', {
    scope: 'guild',
  }),
  p('MANAGE_NICKNAMES', 27, 'membership', {
    scope: 'guild',
  }),
  p('KICK_MEMBERS', 1, 'membership', { scope: 'guild' }),
  p('BAN_MEMBERS', 2, 'membership', {
    scope: 'guild',
  }),
  p('MODERATE_MEMBERS', 40, 'membership', { scope: 'guild' }),

  p('SEND_MESSAGES', 11, 'text', {
    scope: 'text',
  }),
  p('SEND_MESSAGES_IN_THREADS', 38, 'text', { scope: 'text' }),
  p('CREATE_PUBLIC_THREADS', 35, 'text', {
    scope: 'text',
  }),
  p('CREATE_PRIVATE_THREADS', 36, 'text', {
    scope: 'text',
  }),
  p('EMBED_LINKS', 14, 'text', {
    scope: 'text',
  }),
  p('ATTACH_FILES', 15, 'text', { scope: 'text' }),
  p('ADD_REACTIONS', 6, 'text', {
    scope: 'text',
  }),
  p('USE_EXTERNAL_EMOJIS', 18, 'text', {
    scope: 'text',
  }),
  p('USE_EXTERNAL_STICKERS', 37, 'text', { scope: 'text' }),
  p('MENTION_EVERYONE', 17, 'text', { scope: 'text' }),
  p('MANAGE_MESSAGES', 13, 'text', { scope: 'text' }),
  p('PIN_MESSAGES', 51, 'text', { scope: 'text' }),
  p('BYPASS_SLOWMODE', 52, 'text', { scope: 'text' }),
  p('MANAGE_THREADS', 34, 'text', {
    scope: 'text',
  }),
  p('READ_MESSAGE_HISTORY', 16, 'text', { scope: 'text' }),
  p('SEND_TTS_MESSAGES', 12, 'text', {
    scope: 'text',
  }),
  p('SEND_VOICE_MESSAGES', 46, 'text', {
    scope: 'text',
  }),
  p('SEND_POLLS', 49, 'text', { scope: 'text' }),

  p('CONNECT', 20, 'voice', { scope: 'voice' }),
  p('SPEAK', 21, 'voice', { scope: 'voice' }),
  p('STREAM', 9, 'voice', {
    scope: 'voice',
  }),
  p('USE_EMBEDDED_ACTIVITIES', 39, 'voice', {
    scope: 'voice',
  }),
  p('USE_SOUNDBOARD', 42, 'voice', {
    scope: 'voice',
  }),
  p('SET_VOICE_CHANNEL_STATUS', 48, 'voice', { scope: 'voice' }),
  p('USE_EXTERNAL_SOUNDS', 45, 'voice', {
    scope: 'voice',
  }),
  p('USE_VAD', 25, 'voice', { scope: 'voice' }),
  p('PRIORITY_SPEAKER', 8, 'voice', { scope: 'voice' }),
  p('MUTE_MEMBERS', 22, 'voice', {
    scope: 'voice',
  }),
  p('DEAFEN_MEMBERS', 23, 'voice', {
    scope: 'voice',
  }),
  p('MOVE_MEMBERS', 24, 'voice', {
    scope: 'voice',
  }),
  p('REQUEST_TO_SPEAK', 32, 'voice', {
    scope: 'voice',
  }),

  p('USE_APPLICATION_COMMANDS', 31, 'apps', { scope: 'text' }),
  p('USE_EXTERNAL_APPS', 50, 'apps', { scope: 'text' }),

  p('MANAGE_EVENTS', 33, 'events', { scope: 'guild' }),
  p('CREATE_EVENTS', 44, 'events', { scope: 'guild' }),

  p('ADMINISTRATOR', 3, 'advanced', { danger: true, scope: 'guild' }),
  p('VIEW_CREATOR_MONETIZATION_ANALYTICS', 41, 'advanced', { scope: 'guild' }),

  p('START_WATCH_PARTY', 56, 'sideroom'),
  p('CONTROL_WATCH_PARTY', 57, 'sideroom'),
  p('USE_LIBRARY_EMBEDS', 58, 'sideroom', { scope: 'text' }),
  p('MANAGE_EXTENSIONS', 59, 'sideroom', { scope: 'guild' }),
  p('USE_EXPRESSIONS_EXTERNALLY', 60, 'sideroom', { scope: 'guild' }),
];

export const PERM = Object.fromEntries(
  PERMISSIONS.map((x) => [x.id, 1n << BigInt(x.bit)]),
) as Record<string, bigint>;
export const ALL_BITS = (1n << 64n) - 1n;

export const toBig = (s: string | number | bigint | null | undefined): bigint => {
  try {
    return s === null || s === undefined || s === '' ? 0n : BigInt(s);
  } catch {
    return 0n;
  }
};

export const has = (bits: bigint, bit: bigint) => (bits & bit) === bit;

/** Matrix groups, optionally limited to some permissions. */
export function permissionGroups(filter?: (p: PermissionInfo) => boolean): PermissionGroupDef[] {
  return GROUPS.map((g) => ({
    id: g.id,
    label: g.label,
    items: PERMISSIONS.filter((x) => x.group === g.id && (!filter || filter(x))).map((x) => ({
      id: x.id,
      label: x.label,
      description: x.description,
      ...(x.danger ? { danger: true } : {}),
    })),
  })).filter((g) => g.items.length > 0);
}

/** Bitfield -> matrix values (set bits are "allow"). */
export function bitsToValues(bits: bigint): Record<string, TriState> {
  const out: Record<string, TriState> = {};
  for (const x of PERMISSIONS) if (has(bits, PERM[x.id]!)) out[x.id] = 'allow';
  return out;
}

/** Matrix values -> bitfield of allowed permissions. */
export function valuesToBits(values: Record<string, TriState>): bigint {
  let b = 0n;
  for (const x of PERMISSIONS) if (values[x.id] === 'allow') b |= PERM[x.id]!;
  return b;
}

/** Overwrite allow/deny bitfields -> tri-state matrix values. */
export function overwriteToValues(allow: bigint, deny: bigint): Record<string, TriState> {
  const out: Record<string, TriState> = {};
  for (const x of PERMISSIONS) {
    const bit = PERM[x.id]!;
    if (has(allow, bit)) out[x.id] = 'allow';
    else if (has(deny, bit)) out[x.id] = 'deny';
  }
  return out;
}

export function valuesToOverwrite(values: Record<string, TriState>): {
  allow: bigint;
  deny: bigint;
} {
  let allow = 0n;
  let deny = 0n;
  for (const x of PERMISSIONS) {
    if (values[x.id] === 'allow') allow |= PERM[x.id]!;
    else if (values[x.id] === 'deny') deny |= PERM[x.id]!;
  }
  return { allow, deny };
}

/** Names of the permissions set in a bitfield (unknown bits are ignored). */
export const permissionNames = (bits: bigint) =>
  PERMISSIONS.filter((x) => has(bits, PERM[x.id]!)).map((x) => x.label);

/** Does this base permission set open any server settings section? (Mirrors the section gates.) */
export const canOpenSettingsFromBits = (bits: bigint) =>
  [
    'MANAGE_GUILD',
    'MANAGE_ROLES',
    'MANAGE_CHANNELS',
    'BAN_MEMBERS',
    'KICK_MEMBERS',
    'MODERATE_MEMBERS',
    'VIEW_AUDIT_LOG',
    'MANAGE_WEBHOOKS',
    'MANAGE_GUILD_EXPRESSIONS',
  ].some((n) => has(bits, PERM[n]!));
