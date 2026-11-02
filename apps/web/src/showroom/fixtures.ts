// UX-SHOW-001: the Showroom's sample data. This module is reached ONLY through the lazy preview chunks, after sign-in
// (D-385), so it is never part of the entrance or the app shell. Every name is invented: no real person, brand or
// title. Nothing here is ever sent anywhere.

export interface FixtureServer {
  id: string;
  name: string;
  unread: boolean;
  mentions: number;
  /** People in a voice channel right now (0 = nobody). */
  voice: number;
  muted?: boolean;
}

export const FIXTURE_SERVERS: FixtureServer[] = [
  { id: 'fx-1', name: 'Moss Lantern Club', unread: true, mentions: 3, voice: 4 },
  { id: 'fx-2', name: 'Quillstone Annex', unread: true, mentions: 0, voice: 0 },
  { id: 'fx-3', name: 'Paper Boat Cinema', unread: false, mentions: 0, voice: 6 },
  { id: 'fx-4', name: 'Nimbus Workshop', unread: true, mentions: 12, voice: 2 },
  { id: 'fx-5', name: 'Hollow Tide Readers', unread: false, mentions: 0, voice: 0 },
  { id: 'fx-6', name: 'Ember Orchard', unread: true, mentions: 1, voice: 0 },
  { id: 'fx-7', name: 'Tin Whistle Arcade', unread: false, mentions: 0, voice: 9 },
  { id: 'fx-8', name: 'Velvet Compass', unread: false, mentions: 0, voice: 0, muted: true },
  { id: 'fx-9', name: 'Lowlight Library', unread: true, mentions: 0, voice: 1 },
  { id: 'fx-10', name: 'Sundial Sketchers', unread: false, mentions: 0, voice: 3 },
];

export type FixtureKind = 'film' | 'book' | 'comic' | 'music' | 'doc';

export interface FixtureItem {
  id: string;
  title: string;
  kind: FixtureKind;
  bytes: number;
  /** ISO date the item was added to the shelf. */
  added: string;
  /** Local metadata: kept on the device, never uploaded. */
  tags: string[];
  note: string;
}

export interface FixtureShelf {
  id: string;
  source: 'mine' | 'friend' | 'server';
  /** Friend handle or server name; `null` for the viewer's own files. */
  owner: string | null;
  label: string;
  items: FixtureItem[];
}

const MB = 1024 * 1024;
const item = (
  id: string,
  title: string,
  kind: FixtureKind,
  mb: number,
  added: string,
  tags: string[],
  note: string,
): FixtureItem => ({ id, title, kind, bytes: mb * MB, added, tags, note });

export const FIXTURE_SHELVES: FixtureShelf[] = [
  {
    id: 'fx-sh-mine',
    source: 'mine',
    owner: null,
    label: 'My Files',
    items: [
      item(
        'm1',
        'Sample Film: The Quiet Lighthouse',
        'film',
        1420,
        '2026-10-02',
        ['drama'],
        'Watch with the club on Friday.',
      ),
      item(
        'm2',
        'Sample Novel: Salt and Lanterns',
        'book',
        3,
        '2026-09-21',
        ['fiction', 'sea'],
        'Chapter 7 is the good one.',
      ),
      item('m3', 'Sample Album: Late Bus Home', 'music', 88, '2026-08-30', ['ambient'], ''),
    ],
  },
  {
    id: 'fx-sh-f1',
    source: 'friend',
    owner: 'demo_juniper',
    label: 'Juniper shelf',
    items: [
      item(
        'f1a',
        'Sample Comic: Robots of Tuesday #4',
        'comic',
        62,
        '2026-10-12',
        ['sci-fi', 'weekly'],
        'Lends out on request.',
      ),
      item('f1b', 'Sample Film: Paper Moons', 'film', 980, '2026-09-30', ['animation'], ''),
    ],
  },
  {
    id: 'fx-sh-f2',
    source: 'friend',
    owner: 'demo_barnaby',
    label: 'Barnaby shelf',
    items: [
      item('f2a', 'Sample Notes: Garden Planner', 'doc', 1, '2026-10-20', ['notes'], 'Plain text.'),
      item('f2b', 'Sample Album: Kettle Songs', 'music', 120, '2026-10-01', ['folk'], ''),
      item(
        'f2c',
        'Sample Novel: The Cartographer of Nowhere',
        'book',
        4,
        '2026-07-14',
        ['fantasy'],
        '',
      ),
    ],
  },
  {
    id: 'fx-sh-f3',
    source: 'friend',
    owner: 'demo_wren',
    label: 'Wren shelf',
    items: [
      item('f3a', 'Sample Comic: Moth Court Vol. 2', 'comic', 140, '2026-09-02', ['mystery'], ''),
    ],
  },
  {
    id: 'fx-sh-s1',
    source: 'server',
    owner: 'Paper Boat Cinema',
    label: 'Paper Boat Cinema shelf',
    items: [
      item(
        's1a',
        'Sample Film: Harbor Lights',
        'film',
        1710,
        '2026-10-18',
        ['club pick'],
        'Next screening.',
      ),
      item('s1b', 'Sample Film: The Long Orchard', 'film', 1305, '2026-10-05', ['club pick'], ''),
    ],
  },
  {
    id: 'fx-sh-s2',
    source: 'server',
    owner: 'Hollow Tide Readers',
    label: 'Hollow Tide Readers shelf',
    items: [
      item('s2a', 'Sample Novel: Glass Months', 'book', 5, '2026-10-09', ['book club'], ''),
      item('s2b', 'Sample Comic: Lantern Keepers #1', 'comic', 48, '2026-09-27', ['book club'], ''),
    ],
  },
  {
    id: 'fx-sh-s3',
    source: 'server',
    owner: 'Lowlight Library',
    label: 'Lowlight Library shelf',
    items: [
      item('s3a', 'Sample Notes: Reading Order Guide', 'doc', 1, '2026-08-11', ['guide'], ''),
    ],
  },
];

export interface FixtureGame {
  id: string;
  title: string;
  blurb: string;
  /** `live` builds are playable now; `preview` entries are showcase cards for games still in development. */
  status: 'live' | 'preview';
  players: string;
  tags: string[];
}

export const FIXTURE_GAMES: FixtureGame[] = [
  {
    id: 'g1',
    title: 'Doodle Relay (sample)',
    blurb: 'Pass a drawing down the line and guess what it became.',
    status: 'live',
    players: '3 to 8',
    tags: ['party', 'guest_play'],
  },
  {
    id: 'g2',
    title: 'Orbit Pairs (sample)',
    blurb: 'A quick memory game on a spinning board.',
    status: 'live',
    players: '1 to 4',
    tags: ['puzzle', 'ingame_voice'],
  },
  {
    id: 'g3',
    title: 'Tidewords (sample)',
    blurb: 'Build words as the tide rearranges the letters.',
    status: 'live',
    players: '2 to 6',
    tags: ['words', 'lobby_channel'],
  },
  {
    id: 'g4',
    title: 'Lantern Run (sample)',
    blurb: 'Co-op platformer through a cave of lanterns.',
    status: 'preview',
    players: '2 to 4',
    tags: ['co-op'],
  },
  {
    id: 'g5',
    title: 'Courier Cats (sample)',
    blurb: 'Deliver parcels across the rooftops before the rain.',
    status: 'preview',
    players: '1 to 2',
    tags: ['arcade'],
  },
  {
    id: 'g6',
    title: 'Hush Hotel (sample)',
    blurb: 'A whispering mystery for one detective and many suspects.',
    status: 'preview',
    players: '4 to 10',
    tags: ['party', 'mystery'],
  },
  {
    id: 'g7',
    title: 'Plinko Parliament (sample)',
    blurb: 'Vote on rules, then watch the balls decide.',
    status: 'preview',
    players: '3 to 12',
    tags: ['party'],
  },
];
