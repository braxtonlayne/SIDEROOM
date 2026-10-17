// The profile document (EXT-PRF-001) as the client sees it. The server (`sr-packages::profile_doc`) is the
// authority: it validates every document on save and again when a page is served, so these types only
// describe what the editor may build. Lists below mirror the server's closed lists.

export const GRID_COLUMNS = 12;
export const MAX_ROWS = 60;
export const MAX_BLOCKS = 16;

export const BLOCK_KINDS = [
  'header',
  'about',
  'top_friends',
  'now_watching',
  'favorites',
  'gallery',
  'guestbook',
  'song',
  'custom',
] as const;
export type BlockKind = (typeof BLOCK_KINDS)[number];

/** Blocks that can appear once. */
export const SINGLETONS: readonly BlockKind[] = [
  'header',
  'guestbook',
  'top_friends',
  'now_watching',
];

export const FONTS = ['inter', 'newsreader', 'hanken-grotesk', 'plex-mono'] as const;
export const CURSORS = ['default', 'pointer', 'crosshair', 'cell', 'help'] as const;
export const OVERLAYS = ['none', 'sparkle', 'stars', 'hearts', 'snow'] as const;
export const BORDERS = ['none', 'solid', 'dashed', 'dotted', 'double', 'groove'] as const;
export const PATTERNS = ['none', 'dots', 'grid', 'stripes', 'checks', 'waves'] as const;

export interface Palette {
  background: string;
  panel: string;
  text: string;
  accent: string;
}

export interface Style {
  palette: Palette;
  pattern: string;
  banner_background: boolean;
  font: string;
  cursor: string;
  overlay: string;
  border: string;
}

export interface Block {
  id: string;
  type: BlockKind;
  x: number;
  y: number;
  w: number;
  h: number;
  config: Record<string, unknown>;
}

export interface ProfileDoc {
  version: 1;
  layout?: string | null;
  style: Style;
  blocks: Block[];
}

export interface PageUser {
  id: string;
  username: string;
  display_name: string | null;
  avatar: string | null;
  banner: string | null;
  accent_color: number | null;
}

export interface ProfilePage {
  user: PageUser;
  doc: ProfileDoc;
  friends: PageUser[];
  is_owner: boolean;
  can_sign: boolean;
  customised: boolean;
}

export interface GuestbookEntry {
  can_remove: boolean;
  id: string;
  author: PageUser;
  body: string;
  created_at_ms: number;
}

export const DEFAULT_PALETTE: Palette = {
  background: '#1e1f22',
  panel: '#2b2d31',
  text: '#f2f3f5',
  accent: '#5865f2',
};

export const DEFAULT_STYLE: Style = {
  palette: DEFAULT_PALETTE,
  pattern: 'none',
  banner_background: false,
  font: 'inter',
  cursor: 'default',
  overlay: 'none',
  border: 'none',
};

export const emptyDoc = (): ProfileDoc => ({
  version: 1,
  style: { ...DEFAULT_STYLE, palette: { ...DEFAULT_PALETTE } },
  blocks: [],
});

/** Starting size of a new block (columns, rows). */
export const DEFAULT_SIZE: Record<BlockKind, [number, number]> = {
  header: [12, 3],
  about: [6, 4],
  top_friends: [6, 4],
  now_watching: [6, 3],
  favorites: [6, 4],
  gallery: [6, 5],
  guestbook: [6, 6],
  song: [6, 3],
  custom: [6, 4],
};
