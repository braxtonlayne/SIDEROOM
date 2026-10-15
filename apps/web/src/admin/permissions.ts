// Platform permission bits (docs/07 section 3; crates/sr-core/src/platform_permissions.rs).
// The server is the security boundary: this only decides what to show.
export const AP = {
  STAFF_BASIC: 0,
  REVIEW_MEDIA: 1,
  REVIEW_READING: 2,
  PUBLISH_MEDIA: 3,
  PUBLISH_READING: 4,
  DELETE_CONTENT: 5,
  MANAGE_COLLECTIONS: 6,
  MANAGE_TV: 7,
  REVIEW_APPS: 8,
  REVIEW_PACKAGES: 9,
  MANAGE_DIRECTORY: 10,
  MANAGE_ACCOUNTS: 11,
  PLATFORM_BAN: 12,
  MANAGE_INVITES: 13,
  MANAGE_DONATIONS: 14,
  MANAGE_FEATURES: 15,
  READ_PRIVATE_CONTENT: 16,
  MANAGE_GUILDS_GLOBAL: 17,
  VIEW_ACCESS_LOG: 18,
  APPOINT_STAFF: 19,
  MANAGE_TRANSCODE: 20,
  OPERATOR: 21,
  MANAGE_CASES: 22,
  MANAGE_TAKEDOWNS: 23,
  OWNER: 63,
} as const;

export type AdminPerm = keyof typeof AP;

/** Only the Owner may grant these (doc 07 section 4). */
export const OWNER_ONLY_GRANTS: AdminPerm[] = [
  'READ_PRIVATE_CONTENT',
  'APPOINT_STAFF',
  'VIEW_ACCESS_LOG',
  'OPERATOR',
];

export const bit = (p: AdminPerm): bigint => 1n << BigInt(AP[p]);

/** Parse the decimal-string bitfield the API sends; bad input is "no permissions". */
export function parsePerms(s: string | null | undefined): bigint {
  if (!s || !/^\d+$/.test(s)) return 0n;
  return BigInt(s);
}

export const hasPerm = (perms: bigint, p: AdminPerm): boolean =>
  (perms & bit('OWNER')) !== 0n || (perms & bit(p)) !== 0n;

export const hasAny = (perms: bigint, ...ps: AdminPerm[]): boolean =>
  ps.some((p) => hasPerm(perms, p));

const OWNER_ONLY_MASK = OWNER_ONLY_GRANTS.reduce((a, p) => a | bit(p), 0n);

/**
 * May the viewer assign a role with these bits? The Owner may assign anything but the Owner role; a delegate
 * only bits they hold, and never the Owner-only ones.
 */
export function canAssignRole(
  viewer: { isOwner: boolean; perms: bigint },
  roleBits: bigint,
): boolean {
  if ((roleBits & bit('OWNER')) !== 0n) return false;
  if (viewer.isOwner) return true;
  if ((roleBits & OWNER_ONLY_MASK) !== 0n) return false;
  return (viewer.perms & roleBits) === roleBits;
}

/** Names of the permissions set in `bits`, for display. */
export function permNames(bits: bigint): string[] {
  if ((bits & bit('OWNER')) !== 0n) return ['OWNER'];
  return (Object.keys(AP) as AdminPerm[]).filter((p) => p !== 'OWNER' && (bits & bit(p)) !== 0n);
}
