// Typed client for the profile extras that live outside `GET /users/{id}/profile`.
//
// * Lists: `GET /cinema/users/{user_id}/lists` exists (public lists, or the ones shared with friends).
// * Reviews: contract, not built yet: `GET /cinema/users/{user_id}/reviews` answering `{ reviews: UserReview[] }`
//   (newest first, only reviews the viewer may see). While the route is missing the tab shows its empty state.
import { apiFetch } from '../../api/client';

export interface UserList {
  id: string;
  name: string;
  description: string;
  item_count: number;
  kind: string;
}

export const listUserLists = async (userId: string): Promise<UserList[]> =>
  (
    await apiFetch<{ lists: UserList[] }>(`/cinema/users/${encodeURIComponent(userId)}/lists`, {
      retries: 0,
    })
  ).lists;

export interface UserReview {
  id: string;
  title_id: string;
  title: string;
  /** 1 to 5, or 1 to 10 depending on the scale; shown as given. */
  rating?: number | null;
  body?: string | null;
  created_at: string;
}

export const listUserReviews = async (userId: string): Promise<UserReview[]> =>
  (
    await apiFetch<{ reviews: UserReview[] }>(
      `/cinema/users/${encodeURIComponent(userId)}/reviews`,
      { retries: 0 },
    )
  ).reviews;
