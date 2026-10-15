// Server tags (LNG-SSET-011/012): the badge icon is an id 0 to 15 that the client maps to a glyph.
export const TAG_BADGES = [
  '★',
  '♥',
  '♦',
  '♣',
  '♠',
  '☀',
  '☾',
  '⚑',
  '⚔',
  '☘',
  '✿',
  '♫',
  '⚓',
  '⚡',
  '❄',
  '☠',
] as const;

export const badgeGlyph = (badge: number): string => TAG_BADGES[badge] ?? TAG_BADGES[0];

/** 2 to 4 letters or digits, as the server accepts it. */
export const isValidTag = (tag: string): boolean => /^[\p{L}\p{N}]{2,4}$/u.test(tag);

export const hexOfColor = (n: number): string => `#${(n & 0xffffff).toString(16).padStart(6, '0')}`;
export const colorOfHex = (h: string): number => parseInt(h.replace('#', ''), 16) || 0;
