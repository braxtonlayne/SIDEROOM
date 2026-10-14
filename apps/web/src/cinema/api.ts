// Typed fetchers for the Cinema routes (spec sections 8). The only place that knows URLs and wire field names:
// each fetcher adapts the server's shape to the UI types in types.ts. Components never build URLs.
import { apiFetch } from '../api/client';
import type {
  BrowseParams,
  Collection,
  Comment,
  Discover,
  Episode,
  Facets,
  MyCinema,
  Person,
  RatingSummary,
  Review,
  Row,
  Season,
  Title,
  TitleDetail,
  TitleKind,
  TitlePage,
  TrendingScope,
  Variant,
} from './types';

type Query = Record<string, string | number | boolean | undefined | null>;

export function qs(q: Query): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) {
    if (v === undefined || v === null || v === '') continue;
    p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : '';
}

const enc = encodeURIComponent;

// ---- wire shapes (the parts the UI reads) ----
interface WireTitle {
  id: string;
  kind: TitleKind;
  title: string;
  original_title?: string | null;
  year?: number | null;
  runtime_min?: number | null;
  content_rating?: string | null;
  synopsis?: string;
  genres?: string[];
  languages?: string[];
  subtitle_languages?: string[];
  cast?: unknown[];
  directors?: unknown[];
  writers?: unknown[];
  crew?: unknown[];
  studios?: string[];
  age_restricted?: boolean;
  poster_url?: string | null;
  backdrop_url?: string | null;
  logo_url?: string | null;
  season_count?: number;
  has_trailer?: boolean;
  /** Row extras (RowItem). */
  progress_percent?: number | null;
  episode_id?: string | null;
}
interface WireRow {
  key: string;
  kind: 'curated' | 'generated' | 'provided';
  display?: 'row' | 'hero' | 'banner';
  title: string;
  eyebrow?: string | null;
  subtitle?: string | null;
  explore?: string | null;
  items: WireTitle[];
}

export function toTitle(w: WireTitle): Title {
  return {
    id: String(w.id),
    kind: w.kind,
    title: w.title,
    original_title: w.original_title ?? null,
    year: w.year ?? null,
    runtime_min: w.runtime_min ?? null,
    season_count: w.season_count || null,
    content_rating: w.content_rating ?? null,
    genres: w.genres ?? [],
    synopsis: w.synopsis ?? '',
    poster_url: w.poster_url ?? null,
    backdrop_url: w.backdrop_url ?? null,
    logo_url: w.logo_url ?? null,
    has_trailer: w.has_trailer ?? false,
    age_restricted: w.age_restricted ?? false,
    my:
      w.progress_percent != null
        ? { progress_percent: w.progress_percent, episode_id: w.episode_id ?? null }
        : null,
  };
}

const KIND_PATH: Record<string, string> = { movie: 'movies', series: 'series', anime: 'anime' };

/** `explore` is a `GET /cinema/titles` query string; turn it into the matching browse page link. */
export function exploreHref(explore: string | null | undefined): string | null {
  if (!explore) return null;
  const p = new URLSearchParams(explore.replace(/^\?/, ''));
  const kind = p.get('kind') ?? 'movie';
  p.delete('kind');
  p.delete('limit');
  p.delete('cursor');
  const rest = p.toString();
  return `/cinema/${KIND_PATH[kind] ?? 'movies'}${rest ? `?${rest}` : ''}`;
}

export function toRow(w: WireRow): Row {
  return {
    key: w.key,
    source: w.kind,
    display: w.display ?? 'row',
    title: w.title,
    label: w.eyebrow ?? null,
    explore_href: exploreHref(w.explore),
    titles: w.items.map(toTitle),
  };
}

const person = (p: unknown, role?: string): Person =>
  typeof p === 'string'
    ? { name: p, role: role ?? null }
    : {
        name: String((p as { name?: unknown }).name ?? ''),
        role: role ?? (p as { role?: string }).role ?? null,
      };

// ---- catalog ----
export const getDiscover = async (scope: TrendingScope = 'global'): Promise<Discover> => {
  const d = await apiFetch<{ hero: WireRow | null; rows: WireRow[] }>(
    `/cinema/discover${qs({ scope })}`,
  );
  return { hero: (d.hero?.items ?? []).map(toTitle), rows: d.rows.map(toRow) };
};

export const browseTitles = async (params: BrowseParams = {}): Promise<TitlePage> => {
  const p = await apiFetch<{ items: WireTitle[]; total: number; next_cursor?: string | null }>(
    `/cinema/titles${qs({ ...params })}`,
  );
  return { items: p.items.map(toTitle), total: p.total, next_cursor: p.next_cursor ?? null };
};

export const getFacets = (kind?: TitleKind) => apiFetch<Facets>(`/cinema/facets${qs({ kind })}`);

export const getCollections = async (surface?: string): Promise<{ items: Collection[] }> => {
  const c = await apiFetch<{ items: { id: string; name: string }[] }>(
    `/cinema/collections${qs({ surface })}`,
  );
  return { items: c.items.map((x) => ({ id: String(x.id), title: x.name })) };
};

export const getTitle = async (id: string): Promise<TitleDetail> => {
  const d = await apiFetch<{
    title: WireTitle;
    seasons?: { id: string; number: number; name?: string | null; episode_count: number }[];
    variants?: { id: string; label: string; notes?: string | null }[];
  }>(`/cinema/titles/${enc(id)}`);
  const t = d.title;
  const seasons: Season[] = (d.seasons ?? []).map((s) => ({
    id: String(s.id),
    number: s.number,
    title: s.name ?? null,
    episode_count: s.episode_count,
  }));
  // The title itself is the default cut; the variants are the other cuts.
  const variants: Variant[] = (d.variants ?? []).map((v) => ({
    id: String(v.id),
    label: v.label,
    notes: v.notes ?? null,
  }));
  const title = toTitle(t);
  if (t.season_count == null && seasons.length) title.season_count = seasons.length;
  return {
    title,
    cast: (t.cast ?? []).map((p) => person(p)),
    crew: [
      ...(t.directors ?? []).map((p) => person(p, 'Director')),
      ...(t.writers ?? []).map((p) => person(p, 'Writer')),
      ...(t.crew ?? []).map((p) => person(p)),
    ],
    studios: t.studios ?? [],
    audio_languages: t.languages ?? [],
    subtitle_languages: t.subtitle_languages ?? [],
    variants,
    seasons,
    contributors: [],
    patron_download: false,
  };
};

interface WireEpisode {
  id: string;
  season_number: number;
  number: number;
  title: string;
  synopsis?: string | null;
  runtime_min?: number | null;
  still_url?: string | null;
}
interface WireProgress {
  episode_id: string;
  percent: number;
  completed: boolean;
}

/** Episodes of one season with the caller's progress (watch-state) merged in. */
export const getEpisodes = async (id: string, season: number): Promise<{ items: Episode[] }> => {
  const [eps, state] = await Promise.all([
    apiFetch<{ episodes: WireEpisode[] }>(
      `/cinema/titles/${enc(id)}/episodes${qs({ season, limit: 100 })}`,
    ),
    apiFetch<{ items: WireProgress[] }>(`/cinema/titles/${enc(id)}/watch-state`).catch(() => ({
      items: [] as WireProgress[],
    })),
  ]);
  const by = new Map(state.items.map((p) => [String(p.episode_id), p]));
  return {
    items: eps.episodes.map((e) => {
      const p = by.get(String(e.id));
      return {
        id: String(e.id),
        season: e.season_number,
        number: e.number,
        title: e.title,
        synopsis: e.synopsis ?? null,
        runtime_min: e.runtime_min ?? null,
        still_url: e.still_url ?? null,
        progress_percent: p && !p.completed ? p.percent : 0,
        watched: p?.completed ?? false,
      };
    }),
  };
};

/** "More like this": the catalog has no related route yet, so show titles that share the first genre. */
export const getRelated = async (t: Title): Promise<{ items: Title[] }> => {
  const page = await browseTitles({ kind: t.kind, genres: t.genres[0], limit: 13 });
  return { items: page.items.filter((x) => x.id !== t.id).slice(0, 12) };
};

export const searchTitles = async (
  q: string,
  limit = 8,
  signal?: AbortSignal,
  kind?: TitleKind,
): Promise<{ titles: Title[] }> => {
  const r = await apiFetch<{ titles: WireTitle[] }>(`/cinema/search${qs({ q, limit, kind })}`, {
    signal,
  });
  return { titles: r.titles.map(toTitle) };
};

// ---- the caller's own state (progress, lists, ratings; p6-cinema-social) ----
interface WireRef {
  id: string;
}
export const getMyCinema = async (): Promise<MyCinema> => {
  const [list, resume] = await Promise.allSettled([
    apiFetch<{ items: ({ title?: WireRef } & Partial<WireRef>)[] }>(`/cinema/me/list?limit=100`),
    apiFetch<{
      items: {
        reason: string;
        title: WireRef;
        episode?: { id: string } | null;
        progress?: { percent: number; position_secs: number } | null;
      }[];
    }>(`/cinema/me/continue-watching`),
  ]);
  const out: MyCinema = { inList: new Set(), resume: new Map() };
  if (list.status === 'fulfilled')
    for (const it of list.value.items) {
      const id = it.title?.id ?? it.id;
      if (id) out.inList.add(String(id));
    }
  if (resume.status === 'fulfilled')
    for (const it of resume.value.items)
      if (it.reason === 'resume' && it.progress)
        out.resume.set(String(it.title.id), {
          percent: it.progress.percent,
          position_s: it.progress.position_secs,
          episode_id: it.episode ? String(it.episode.id) : null,
        });
  return out;
};

export const setInList = (id: string, on: boolean) =>
  apiFetch<void>(`/cinema/me/list/${enc(id)}`, { method: on ? 'PUT' : 'DELETE', retries: 0 });

/** Mark a whole title (no scope), one episode or one season watched or unwatched. */
export const setWatched = (
  titleId: string,
  watched: boolean,
  scope: { episode_id?: string; season?: number } = {},
) =>
  apiFetch<void>(`/cinema/titles/${enc(titleId)}/watched`, {
    method: 'POST',
    json: { watched, ...scope },
    retries: 0,
  });

export const getRatings = async (id: string): Promise<RatingSummary> => {
  const r = await apiFetch<{
    mine?: number | { score: number } | null;
    global?: { average?: number | null; count: number };
  }>(`/cinema/titles/${enc(id)}/ratings`);
  const mine = typeof r.mine === 'object' && r.mine ? r.mine.score : (r.mine ?? null);
  return {
    mine: typeof mine === 'number' ? mine : null,
    global: { average: r.global?.average ?? null, count: r.global?.count ?? 0 },
  };
};

export const setRating = (id: string, score: number | null) =>
  score === null
    ? apiFetch<void>(`/cinema/titles/${enc(id)}/rating`, { method: 'DELETE', retries: 0 })
    : apiFetch<void>(`/cinema/titles/${enc(id)}/rating`, {
        method: 'PUT',
        json: { score },
        retries: 0,
      });

// ---- reviews and comments ----
export const getReviews = async (id: string, sort: 'helpful' | 'recent' = 'helpful') => {
  const r = await apiFetch<{ items?: Review[]; reviews?: Review[] }>(
    `/cinema/titles/${enc(id)}/reviews${qs({ sort })}`,
  );
  return r.items ?? r.reviews ?? [];
};
export const putReview = (id: string, body: { body: string; spoiler: boolean; rating?: number }) =>
  apiFetch<Review>(`/cinema/titles/${enc(id)}/review`, { method: 'PUT', json: body, retries: 0 });
export const deleteReview = (id: string) =>
  apiFetch<void>(`/cinema/titles/${enc(id)}/review`, { method: 'DELETE', retries: 0 });
export const setReviewHelpful = (reviewId: string, on: boolean) =>
  apiFetch<void>(`/cinema/reviews/${enc(reviewId)}/helpful`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });

export const getComments = async (id: string) => {
  const r = await apiFetch<{ items?: Comment[]; comments?: Comment[] }>(
    `/cinema/titles/${enc(id)}/comments`,
  );
  return r.items ?? r.comments ?? [];
};
export const postComment = (id: string, body: { body: string; spoiler: boolean }) =>
  apiFetch<Comment>(`/cinema/titles/${enc(id)}/comments`, {
    method: 'POST',
    json: body,
    retries: 0,
  });
export const deleteComment = (commentId: string) =>
  apiFetch<void>(`/cinema/comments/${enc(commentId)}`, { method: 'DELETE', retries: 0 });
export const setCommentLike = (commentId: string, on: boolean) =>
  apiFetch<void>(`/cinema/comments/${enc(commentId)}/like`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });

export const keys = {
  discover: (scope: TrendingScope) => ['cinema', 'discover', scope] as const,
  browse: (p: BrowseParams, q?: string) => ['cinema', 'browse', p, q ?? null] as const,
  facets: (kind?: string) => ['cinema', 'facets', kind ?? null] as const,
  collections: (surface?: string) => ['cinema', 'collections', surface ?? null] as const,
  title: (id: string) => ['cinema', 'title', id] as const,
  episodes: (id: string, season: number) => ['cinema', 'episodes', id, season] as const,
  related: (id: string) => ['cinema', 'related', id] as const,
  search: (q: string) => ['cinema', 'search', q] as const,
  my: () => ['cinema', 'my'] as const,
  ratings: (id: string) => ['cinema', 'ratings', id] as const,
  reviews: (id: string, sort: string) => ['cinema', 'reviews', id, sort] as const,
  comments: (id: string) => ['cinema', 'comments', id] as const,
  me: () => ['cinema', 'session'] as const,
};
