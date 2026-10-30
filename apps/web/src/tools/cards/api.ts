import { ApiError, apiFetch } from '../../api/client';

/** An invite card as `GET /channels/{id}/cards/{id}` answers it (UTL-CRD-001). */
export interface InviteCard {
  id: string;
  channel_id: string;
  guild_id: string | null;
  kind: 'cinema' | 'live_tv' | 'game_night';
  title: string;
  note: string;
  starts_at: number;
  duration_min: number;
  timezone: string;
  seats: number;
  taken: number;
  title_id: string | null;
  link_path: string | null;
  created_by: string | null;
  status: 'open' | 'started' | 'closed' | 'canceled';
  claimants: { user_id: string; claimed_at: number }[];
  mine: { remind: boolean; reminder_deliverable: boolean } | null;
  can_start_party: boolean;
  can_manage: boolean;
}

export interface NewInviteCard {
  kind: InviteCard['kind'];
  title: string;
  note?: string;
  starts_at: number;
  duration_min?: number;
  timezone?: string;
  seats: number;
  title_id?: string;
  link_path?: string;
}

const base = (channelId: string) => `/channels/${encodeURIComponent(channelId)}/cards`;

export const createCard = (channelId: string, body: NewInviteCard) =>
  apiFetch<InviteCard>(base(channelId), { method: 'POST', retries: 0, json: body });

export const getCard = (channelId: string, cardId: string) =>
  apiFetch<InviteCard>(`${base(channelId)}/${encodeURIComponent(cardId)}`);

/** Take a seat; asking again only changes the reminder choice. */
export const claimSeat = (channelId: string, cardId: string, remind: boolean) =>
  apiFetch<InviteCard>(`${base(channelId)}/${encodeURIComponent(cardId)}/claim`, {
    method: 'POST',
    retries: 0,
    json: { remind },
  });

export const unclaimSeat = (channelId: string, cardId: string) =>
  apiFetch<InviteCard>(`${base(channelId)}/${encodeURIComponent(cardId)}/claim`, {
    method: 'DELETE',
    retries: 0,
  });

export const cancelCard = (channelId: string, cardId: string) =>
  apiFetch<void>(`${base(channelId)}/${encodeURIComponent(cardId)}`, {
    method: 'DELETE',
    retries: 0,
  });

/** The `.ics` file of a card. */
export async function fetchIcs(channelId: string, cardId: string): Promise<Blob> {
  const res = await fetch(`/api/v1${base(channelId)}/${encodeURIComponent(cardId)}/ics`, {
    credentials: 'include',
  });
  if (!res.ok) throw new ApiError(res.status, res.statusText, null);
  return res.blob();
}

/** Hands a blob to the browser as a download. */
export function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export interface PartyRef {
  id: string;
}

/** Start a watch party for the card's title in its channel; the server checks `START_WATCH_PARTY`. */
export const startParty = (channelId: string, titleId: string) =>
  apiFetch<PartyRef>('/watch-parties', {
    method: 'POST',
    retries: 0,
    json: { title_id: titleId, channel_id: channelId },
  });

/** The party already running in a channel. */
export const partyInChannel = (channelId: string) =>
  apiFetch<PartyRef>(`/channels/${encodeURIComponent(channelId)}/watch-party`);
