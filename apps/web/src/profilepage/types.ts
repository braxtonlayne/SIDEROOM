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
  'linktree',
  'featured_guilds',
  'now_playing',
] as const;
export type BlockKind = (typeof BLOCK_KINDS)[number];

/** Blocks that can appear once. */
export const SINGLETONS: readonly BlockKind[] = [
  'header',
  'guestbook',
  'top_friends',
  'now_watching',
  'linktree',
  'featured_guilds',
  'now_playing',
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

/** The page background (K-17): a pattern, my profile banner, or a picture I uploaded. */
export interface Background {
  kind: 'pattern' | 'banner' | 'image';
  pattern?: string;
  image_id?: string;
}

export interface Style {
  palette: Palette;
  /** The older form of the background; `background` wins when present. */
  pattern: string;
  banner_background: boolean;
  background?: Background;
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

/** A picture the person uploaded for their page (K-17). */
export interface ProfileMedia {
  id: string;
  kind: 'background' | 'gallery';
  /** Where it is served from (`/cdn/avatars/<user>/<hash>.<ext>`). */
  url: string;
  content_type: string;
  width: number;
  height: number;
  bytes: number;
  created_at_ms: number;
}

export interface ProfilePage {
  user: PageUser;
  doc: ProfileDoc;
  friends: PageUser[];
  is_owner: boolean;
  can_sign: boolean;
  customised: boolean;
  /** The pictures the document names. */
  media?: ProfileMedia[];
  /** The servers a Featured servers block shows this viewer (PRF-PRO-001). */
  featured_guilds?: { id: string; name: string; icon: string | null }[];
  /** Entries on the guestbook; only the owner is told (K-18). */
  guestbook_count?: number | null;
  guestbook_limit?: number;
}

/** A gallery entry: an address on the web or one of my uploaded pictures (K-16, K-17). */
export interface GalleryItem {
  url?: string;
  image_id?: string;
  caption?: string;
  /** Signed image-proxy path the server fills for a `url` picture (D-259); the only form the client loads. */
  proxy_url?: string;
}

/** The background a style asks for, reading the older `pattern` plus `banner_background` pair when needed. */
export function effectiveBackground(style: Style): Background {
  if (style.background) return style.background;
  return style.banner_background
    ? { kind: 'banner', pattern: style.pattern }
    : { kind: 'pattern', pattern: style.pattern };
}

export interface GuestbookEntry {
  can_remove: boolean;
  id: string;
  /** Null for a note left by a visitor without an account (PRF-PRO-001). */
  author: PageUser | null;
  /** Waiting for the owner's approval; only the owner is ever shown one. */
  pending?: boolean;
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
  linktree: [6, 4],
  featured_guilds: [6, 3],
  now_playing: [6, 3],
};
