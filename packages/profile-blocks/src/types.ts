// The slice of a profile page the renderer needs. Structural on purpose: the app's `ProfilePage` and the
// public page both satisfy it, and nothing here imports app code (D-385).

export interface PageUserLike {
  id: string;
  username: string;
  display_name: string | null;
  avatar: string | null;
  banner: string | null;
}

export interface PaletteLike {
  background: string;
  panel: string;
  text: string;
  accent: string;
}

export interface BackgroundLike {
  kind: string;
  pattern?: string;
  image_id?: string;
}

export interface StyleLike {
  palette: PaletteLike;
  pattern: string;
  banner_background: boolean;
  background?: BackgroundLike;
  font: string;
  cursor: string;
  overlay: string;
  border: string;
}

export interface BlockLike {
  id: string;
  type: string;
  x: number;
  y: number;
  w: number;
  h: number;
  config: Record<string, unknown>;
}

export interface DocLike {
  style: StyleLike;
  blocks: BlockLike[];
}

export interface MediaLike {
  id: string;
  url: string;
}

export interface FeaturedGuildLike {
  id: string;
  name: string;
  icon: string | null;
}

export interface PageLike {
  user: PageUserLike;
  doc: DocLike;
  friends: PageUserLike[];
  featured_guilds?: FeaturedGuildLike[];
  media?: MediaLike[] | null;
}

export type Translate = (key: string, values?: Record<string, string | number>) => string;

/** URL of a stored image (`/cdn/...`), or null when there is no hash. The host owns the rule. */
export type CdnResolver = (
  kind: 'avatars' | 'icons',
  ownerId: string,
  hash: string | null | undefined,
  size: number,
) => string | null;
