// REST client for guild scheduled events (crates/sr-api/src/routes/settings/scheduled_events.rs).
import { apiFetch } from '../../api/client';

/** Discord's `guild_scheduled_event_status`. */
export const EVENT_STATUS = { scheduled: 1, active: 2, completed: 3, canceled: 4 } as const;
/** Discord's `guild_scheduled_event_entity_type`. */
export const EVENT_ENTITY = { stage: 1, voice: 2, external: 3 } as const;

export interface ScheduledEvent {
  id: string;
  guild_id: string;
  channel_id: string | null;
  creator_id: string;
  creator?: { id: string; username?: string; global_name?: string | null };
  name: string;
  description: string | null;
  scheduled_start_time: string;
  scheduled_end_time: string | null;
  status: number;
  entity_type: number;
  entity_metadata: { location?: string | null } | null;
  user_count?: number;
  /** The viewer is interested. */
  user_rsvp?: boolean;
}

export interface CreateEventBody {
  name: string;
  description?: string;
  channel_id?: string;
  entity_type: number;
  entity_metadata?: { location: string };
  scheduled_start_time: string;
  scheduled_end_time?: string;
  privacy_level: 2;
}

export interface ModifyEventBody {
  name?: string;
  description?: string | null;
  status?: number;
}

const base = (guildId: string) => `/guilds/${encodeURIComponent(guildId)}/scheduled-events`;

export const listEvents = (guildId: string) =>
  apiFetch<ScheduledEvent[]>(`${base(guildId)}?with_user_count=true`, { retries: 0 });

export const createEvent = (guildId: string, body: CreateEventBody) =>
  apiFetch<ScheduledEvent>(base(guildId), { method: 'POST', json: body, retries: 0 });

export const modifyEvent = (guildId: string, id: string, body: ModifyEventBody) =>
  apiFetch<ScheduledEvent>(`${base(guildId)}/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

export const deleteEvent = (guildId: string, id: string) =>
  apiFetch<void>(`${base(guildId)}/${encodeURIComponent(id)}`, { method: 'DELETE', retries: 0 });

export const setInterested = (guildId: string, id: string, interested: boolean) =>
  apiFetch<void>(`${base(guildId)}/${encodeURIComponent(id)}/users/@me`, {
    method: interested ? 'PUT' : 'DELETE',
    ...(interested ? { json: {} } : {}),
    retries: 0,
  });
