import { ApiError, apiFetch } from '../../api/client';

/** A tool room as `GET /channels/{id}/tool-rooms/{id}` answers it (UTL-COL-001). */
export interface ToolRoom {
  id: string;
  channel_id: string;
  guild_id: string | null;
  kind: 'tierlist' | 'poll';
  title: string;
  state: Record<string, unknown>;
  version: number;
  created_by: string | null;
  created_at: number;
  expires_at: number;
}

/** The `TOOL_ROOM_UPDATE` dispatch. */
export interface ToolRoomEvent {
  id: string;
  channel_id: string;
  guild_id: string | null;
  kind: string;
  version: number;
  title: string;
  actor_id: string;
  created?: boolean;
  deleted?: boolean;
  /** The keys of an edit (`null` removes one). Absent on create (then `state` is the whole state) and delete. */
  set?: Record<string, unknown>;
  state?: Record<string, unknown>;
}

/** The server surface the sync needs; a fake in tests. */
export interface RoomApi {
  get(channelId: string, roomId: string): Promise<ToolRoom>;
  patch(
    channelId: string,
    roomId: string,
    body: { base_version: number; set: Record<string, unknown>; title?: string },
  ): Promise<ToolRoom>;
}

const base = (channelId: string) => `/channels/${encodeURIComponent(channelId)}/tool-rooms`;

export const listRooms = (channelId: string) =>
  apiFetch<ToolRoom[]>(base(channelId), { retries: 0 });

export const createRoom = (
  channelId: string,
  body: { kind: ToolRoom['kind']; title?: string; state?: Record<string, unknown> },
) => apiFetch<ToolRoom>(base(channelId), { method: 'POST', json: body, retries: 0 });

export const deleteRoom = (channelId: string, roomId: string) =>
  apiFetch<void>(`${base(channelId)}/${encodeURIComponent(roomId)}`, {
    method: 'DELETE',
    retries: 0,
  });

/** A 409 carries the room as it is now in `current`. */
export class StaleRoom extends Error {
  constructor(public readonly current: ToolRoom) {
    super('stale');
  }
}

export const httpRoomApi: RoomApi = {
  get: (channelId, roomId) =>
    apiFetch<ToolRoom>(`${base(channelId)}/${encodeURIComponent(roomId)}`, { retries: 0 }),
  async patch(channelId, roomId, body) {
    try {
      return await apiFetch<ToolRoom>(`${base(channelId)}/${encodeURIComponent(roomId)}`, {
        method: 'PATCH',
        json: body,
        retries: 0,
      });
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        const cur = (e.body as { current?: ToolRoom } | null)?.current;
        if (cur) throw new StaleRoom(cur);
      }
      throw e;
    }
  },
};
