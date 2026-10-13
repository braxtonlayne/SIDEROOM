// Permission catalog for the server settings screens (docs/07 section 2, sr-core permissions.rs).
// Bit numbers match Discord for 0-52; SIDE ROOM adds 56-59. Bitfields travel as decimal strings.
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
  { id: 'general', label: 'General' },
  { id: 'membership', label: 'Membership' },
  { id: 'text', label: 'Text' },
  { id: 'voice', label: 'Voice' },
  { id: 'apps', label: 'Apps' },
  { id: 'events', label: 'Events' },
  { id: 'advanced', label: 'Advanced' },
  { id: 'sideroom', label: 'SIDE ROOM' },
] as const;

const p = (
  id: string,
  bit: number,
  label: string,
  group: string,
  description: string,
  extra: Partial<PermissionInfo> = {},
): PermissionInfo => ({ id, bit, label, group, description, ...extra });

export const PERMISSIONS: PermissionInfo[] = [
  p(
    'VIEW_CHANNEL',
    10,
    'View Channels',
    'general',
    'Members can see channels by default. Turn this off on a role to hide channels from it.',
  ),
  p(
    'MANAGE_CHANNELS',
    4,
    'Manage Channels',
    'general',
    'Create, edit and delete channels and categories.',
    { scope: 'guild' },
  ),
  p(
    'MANAGE_ROLES',
    28,
    'Manage Roles',
    'general',
    'Create new roles and edit or delete roles below their own highest role.',
  ),
  p(
    'MANAGE_GUILD_EXPRESSIONS',
    30,
    'Manage Expressions',
    'general',
    'Edit and delete custom emoji and stickers.',
    { scope: 'guild' },
  ),
  p(
    'CREATE_GUILD_EXPRESSIONS',
    43,
    'Create Expressions',
    'general',
    'Upload custom emoji and stickers.',
    { scope: 'guild' },
  ),
  p('VIEW_AUDIT_LOG', 7, 'View Audit Log', 'general', 'See who changed what in this server.', {
    scope: 'guild',
  }),
  p(
    'VIEW_GUILD_INSIGHTS',
    19,
    'View Server Insights',
    'general',
    'See server insights such as growth and activity.',
    { scope: 'guild' },
  ),
  p('MANAGE_WEBHOOKS', 29, 'Manage Webhooks', 'general', 'Create, edit and delete webhooks.'),
  p(
    'MANAGE_GUILD',
    5,
    'Manage Server',
    'general',
    'Change the server name, description and settings, and manage invites and AutoMod.',
    { scope: 'guild' },
  ),

  p('CREATE_INSTANT_INVITE', 0, 'Create Invite', 'membership', 'Invite new people to this server.'),
  p('CHANGE_NICKNAME', 26, 'Change Nickname', 'membership', 'Change their own nickname.', {
    scope: 'guild',
  }),
  p('MANAGE_NICKNAMES', 27, 'Manage Nicknames', 'membership', 'Change other members’ nicknames.', {
    scope: 'guild',
  }),
  p(
    'KICK_MEMBERS',
    1,
    'Kick Members',
    'membership',
    'Remove members from the server. They can rejoin with a new invite.',
    { scope: 'guild' },
  ),
  p('BAN_MEMBERS', 2, 'Ban Members', 'membership', 'Permanently ban members from the server.', {
    scope: 'guild',
  }),
  p(
    'MODERATE_MEMBERS',
    40,
    'Timeout Members',
    'membership',
    'Stop members from sending messages, reacting or speaking for a set time.',
    { scope: 'guild' },
  ),

  p('SEND_MESSAGES', 11, 'Send Messages', 'text', 'Post messages in text channels.', {
    scope: 'text',
  }),
  p(
    'SEND_MESSAGES_IN_THREADS',
    38,
    'Send Messages in Threads',
    'text',
    'Post messages in threads.',
    { scope: 'text' },
  ),
  p('CREATE_PUBLIC_THREADS', 35, 'Create Public Threads', 'text', 'Start public threads.', {
    scope: 'text',
  }),
  p('CREATE_PRIVATE_THREADS', 36, 'Create Private Threads', 'text', 'Start invite-only threads.', {
    scope: 'text',
  }),
  p('EMBED_LINKS', 14, 'Embed Links', 'text', 'Links posted by members show an embedded preview.', {
    scope: 'text',
  }),
  p('ATTACH_FILES', 15, 'Attach Files', 'text', 'Upload files and media.', { scope: 'text' }),
  p('ADD_REACTIONS', 6, 'Add Reactions', 'text', 'Add new emoji reactions to a message.', {
    scope: 'text',
  }),
  p('USE_EXTERNAL_EMOJIS', 18, 'Use External Emoji', 'text', 'Use emoji from other servers.', {
    scope: 'text',
  }),
  p(
    'USE_EXTERNAL_STICKERS',
    37,
    'Use External Stickers',
    'text',
    'Use stickers from other servers.',
    { scope: 'text' },
  ),
  p(
    'MENTION_EVERYONE',
    17,
    'Mention @everyone, @here and All Roles',
    'text',
    'Ping everyone, everyone online, or any role.',
    { scope: 'text' },
  ),
  p(
    'MANAGE_MESSAGES',
    13,
    'Manage Messages',
    'text',
    'Delete other members’ messages and remove embeds.',
    { scope: 'text' },
  ),
  p('PIN_MESSAGES', 51, 'Pin Messages', 'text', 'Pin and unpin messages.', { scope: 'text' }),
  p(
    'BYPASS_SLOWMODE',
    52,
    'Bypass Slowmode',
    'text',
    'Send messages without waiting for slowmode.',
    { scope: 'text' },
  ),
  p('MANAGE_THREADS', 34, 'Manage Threads', 'text', 'Rename, delete, close and lock threads.', {
    scope: 'text',
  }),
  p(
    'READ_MESSAGE_HISTORY',
    16,
    'Read Message History',
    'text',
    'Read messages sent before the member joined the channel.',
    { scope: 'text' },
  ),
  p('SEND_TTS_MESSAGES', 12, 'Send Text-to-Speech Messages', 'text', 'Send /tts messages.', {
    scope: 'text',
  }),
  p('SEND_VOICE_MESSAGES', 46, 'Send Voice Messages', 'text', 'Post voice messages.', {
    scope: 'text',
  }),
  p('SEND_POLLS', 49, 'Create Polls', 'text', 'Start polls.', { scope: 'text' }),

  p('CONNECT', 20, 'Connect', 'voice', 'Join voice channels.', { scope: 'voice' }),
  p('SPEAK', 21, 'Speak', 'voice', 'Talk in voice channels.', { scope: 'voice' }),
  p('STREAM', 9, 'Video', 'voice', 'Share video, screen or a game in a voice channel.', {
    scope: 'voice',
  }),
  p('USE_EMBEDDED_ACTIVITIES', 39, 'Use Activities', 'voice', 'Start and join activities.', {
    scope: 'voice',
  }),
  p('USE_SOUNDBOARD', 42, 'Use Soundboard', 'voice', 'Play sounds from the soundboard.', {
    scope: 'voice',
  }),
  p('USE_EXTERNAL_SOUNDS', 45, 'Use External Sounds', 'voice', 'Play sounds from other servers.', {
    scope: 'voice',
  }),
  p('USE_VAD', 25, 'Use Voice Activity', 'voice', 'Talk without push-to-talk.', { scope: 'voice' }),
  p(
    'PRIORITY_SPEAKER',
    8,
    'Priority Speaker',
    'voice',
    'Lower the volume of others while speaking.',
    { scope: 'voice' },
  ),
  p('MUTE_MEMBERS', 22, 'Mute Members', 'voice', 'Mute other members in voice.', {
    scope: 'voice',
  }),
  p('DEAFEN_MEMBERS', 23, 'Deafen Members', 'voice', 'Deafen other members in voice.', {
    scope: 'voice',
  }),
  p('MOVE_MEMBERS', 24, 'Move Members', 'voice', 'Move members between voice channels.', {
    scope: 'voice',
  }),
  p('REQUEST_TO_SPEAK', 32, 'Request to Speak', 'voice', 'Ask to speak in stage channels.', {
    scope: 'voice',
  }),

  p(
    'USE_APPLICATION_COMMANDS',
    31,
    'Use Application Commands',
    'apps',
    'Use slash commands and other app commands.',
    { scope: 'text' },
  ),
  p(
    'USE_EXTERNAL_APPS',
    50,
    'Use External Apps',
    'apps',
    'Use apps that were not added to this server.',
    { scope: 'text' },
  ),

  p('MANAGE_EVENTS', 33, 'Manage Events', 'events', 'Edit and cancel events.', { scope: 'guild' }),
  p('CREATE_EVENTS', 44, 'Create Events', 'events', 'Create scheduled events.', { scope: 'guild' }),

  p(
    'ADMINISTRATOR',
    3,
    'Administrator',
    'advanced',
    'Grants every permission and bypasses all channel overwrites. Dangerous: only give this to people you trust.',
    { danger: true, scope: 'guild' },
  ),
  p(
    'VIEW_CREATOR_MONETIZATION_ANALYTICS',
    41,
    'View Creator Analytics',
    'advanced',
    'See creator monetization analytics.',
    { scope: 'guild' },
  ),

  p(
    'START_WATCH_PARTY',
    56,
    'Start Watch Party',
    'sideroom',
    'Start a watch party in this channel.',
  ),
  p(
    'CONTROL_WATCH_PARTY',
    57,
    'Control Watch Party',
    'sideroom',
    'Control playback in someone else’s party (when the host allows shared control).',
  ),
  p(
    'USE_LIBRARY_EMBEDS',
    58,
    'Use Library Embeds',
    'sideroom',
    'Post rich Cinema and Library cards.',
    { scope: 'text' },
  ),
  p(
    'MANAGE_EXTENSIONS',
    59,
    'Manage Extensions',
    'sideroom',
    'Enable or disable approved extensions and themes for the server.',
    { scope: 'guild' },
  ),
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
