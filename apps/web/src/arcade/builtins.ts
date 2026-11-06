// ARC-UI-010: the built-in games the Arcade shows up front. The games themselves are the starter packs in
// `packages/starter-packs/activities/*` and start from a channel (the + menu or the call bar), not from here; this
// list is what the Arcade draws for them: name, pitch, player count, capabilities (copied from each `sideroom.json`)
// and which art to paint.
//
// Adding a built-in game (Sketch & Slander, for one) is one entry below plus its `arcade.builtin.<id>.name` and
// `.pitch` strings. `status: 'preview'` puts it under Coming soon until it ships; an `art` the Arcade has no
// painting for falls back to the generic art, so a new game never breaks the page.

import type { Tier } from './types';

export type BuiltinArt = 'landio' | 'arena3d' | 'holdem' | 'whiteboard' | 'tabletop' | 'sketch';

export interface BuiltinGame {
  /** Stable id: names the i18n keys (`arcade.builtin.<id>.*`) and the card. */
  id: string;
  /** The starter pack's manifest id (`sideroom.json` `id`). */
  packageId: string;
  /** The manifest `name`: how the Activities shelf lists it, so Play can preselect it in the launcher. */
  shelfName: string;
  /** The manifest's content tier, only where the manifest declares one. */
  tier?: Tier;
  /** Shown large at the top of the Arcade (ARC-UI-010, the concept's two hero cards). */
  hero?: boolean;
  status: 'live' | 'preview';
  players?: { min: number; max: number };
  capabilities: readonly string[];
  art: BuiltinArt | string;
}

export const BUILTIN_GAMES: readonly BuiltinGame[] = [
  {
    id: 'landio',
    packageId: 'side-room-landio',
    shelfName: 'Land.io',
    tier: 'TIER_1_ORIGINAL',
    hero: true,
    status: 'live',
    players: { min: 1, max: 24 },
    capabilities: ['datachannel', 'storage', 'guest_play', 'lobby_channel', 'host_handoff'],
    art: 'landio',
  },
  {
    id: 'arena3d',
    packageId: 'side-room-arena3d',
    shelfName: 'Arena 3D',
    tier: 'TIER_1_ORIGINAL',
    hero: true,
    status: 'live',
    players: { min: 1, max: 8 },
    capabilities: [
      'datachannel',
      'storage',
      'guest_play',
      'lobby_channel',
      'byos_read',
      'host_handoff',
    ],
    art: 'arena3d',
  },
  {
    id: 'holdem',
    packageId: 'side-room-holdem',
    shelfName: "Card table: Texas Hold'em",
    status: 'live',
    players: { min: 2, max: 9 },
    capabilities: ['datachannel'],
    art: 'holdem',
  },
  {
    id: 'whiteboard',
    packageId: 'side-room-whiteboard',
    shelfName: 'Whiteboard',
    status: 'live',
    capabilities: ['datachannel'],
    art: 'whiteboard',
  },
  {
    id: 'tabletop',
    packageId: 'side-room-tabletop',
    shelfName: 'Tabletop',
    status: 'live',
    capabilities: ['datachannel'],
    art: 'tabletop',
  },
  {
    id: 'sketch',
    packageId: 'side-room-sketch',
    shelfName: 'Sketch & Slander',
    status: 'live',
    players: { min: 2, max: 16 },
    capabilities: [
      'datachannel',
      'storage',
      'guest_play',
      'lobby_channel',
      'host_handoff',
      'byos_read',
    ],
    art: 'sketch',
  },
];

/** The built-ins split into the two Arcade shelves. */
export function builtinsByStatus(list: readonly BuiltinGame[] = BUILTIN_GAMES): {
  live: BuiltinGame[];
  preview: BuiltinGame[];
} {
  return {
    live: list.filter((g) => g.status === 'live'),
    preview: list.filter((g) => g.status === 'preview'),
  };
}

/** One of the two large cards at the top of the Arcade (D-477). */
export type HeroSlot<P> = { kind: 'pick'; pick: P } | { kind: 'builtin'; game: BuiltinGame };

/**
 * The two large cards (D-477): the Operator's Featured picks at positions 1 and 2 that play in the browser, then the
 * shipped heroes (Land.io, then Arena 3D) for any slot left. A pick that is the same game as a built-in takes that
 * built-in's slot instead of showing it twice: same application id as the shelf entry the built-in's `shelfName`
 * names (when the shelf is known), else the same name. Fixed order, no rotation.
 */
export function heroSlots<
  P extends {
    application_id: string;
    name: string;
    position: number;
    eligibility: { browser_play: boolean };
  },
>(
  featured: readonly P[],
  games: readonly BuiltinGame[] = BUILTIN_GAMES,
  shelfIds: Readonly<Record<string, string>> = {},
): HeroSlot<P>[] {
  const picks = [...featured]
    .sort((a, b) => a.position - b.position)
    .slice(0, 2)
    .filter((p) => p.eligibility.browser_play);
  const sameGame = (g: BuiltinGame, p: P) =>
    shelfIds[g.shelfName] ? shelfIds[g.shelfName] === p.application_id : g.shelfName === p.name;
  const slots: HeroSlot<P>[] = picks.map((pick) => ({ kind: 'pick', pick }));
  for (const game of games.filter((g) => g.hero && g.status === 'live')) {
    if (slots.length >= 2) break;
    if (picks.some((p) => sameGame(game, p))) continue;
    slots.push({ kind: 'builtin', game });
  }
  return slots;
}

/** The painting of a running game (D-484, D-470): the built-in's whose `shelfName` matches, else the generic art. */
export function builtinArtFor(
  applicationName: string,
  games: readonly BuiltinGame[] = BUILTIN_GAMES,
): string {
  return games.find((g) => g.shelfName === applicationName)?.art ?? 'featured';
}

/**
 * How many of the Arcade's lobbies run a game (D-487): by the shelf name for a built-in, by the application id for a
 * Featured pick. Counted on the client from the strip's own list; no API carries it.
 */
export function liveCount(
  lobbies: readonly { application_name: string; application_id: string }[],
  game: { name?: string; applicationId?: string },
): number {
  return lobbies.filter(
    (l) =>
      (game.applicationId !== undefined && l.application_id === game.applicationId) ||
      (game.name !== undefined && l.application_name === game.name),
  ).length;
}
