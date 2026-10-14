// Wire shapes assumed for the personal Cinema routes (CIN-MY-001; the backend is CIN-MY-002/CIN-SOC, owned by
// p6-cinema-social). Every route is called from api.ts only.
export type CinemaKind = 'movie' | 'series' | 'anime';

export interface TitleCard {
  id: string;
  title: string;
  kind: CinemaKind;
  year?: number | null;
  poster_url?: string | null;
  age_restricted?: boolean;
}

export interface ContinueItem {
  title: TitleCard;
  episode_id: string | null;
  episode_label?: string | null;
  position_s: number;
  duration_s: number;
  updated_at_ms: number;
}

export type ListVisibility = 'private' | 'friends' | 'everyone';
export interface CinemaList {
  id: string;
  name: string;
  visibility: ListVisibility;
  item_count: number;
}

export interface HistoryItem {
  id: string;
  title: TitleCard;
  episode_id: string | null;
  episode_label?: string | null;
  watched_at_ms: number;
  completed: boolean;
}
export interface HistoryPage {
  items: HistoryItem[];
  /** History recording is paused for this account. */
  paused: boolean;
}
