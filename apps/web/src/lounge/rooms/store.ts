import { create } from 'zustand';
import { newer, parseRoomUpdate, type RoomUpdate } from './cursor';

/** The latest cursor of every listening room the session has heard of, and the room this member listens to. */
interface RoomsState {
  rooms: Record<string, RoomUpdate>;
  /** Channel id of the room the member joined, or null. */
  listening: string | null;
}

export const useListeningRooms = create<RoomsState>(() => ({ rooms: {}, listening: null }));

/** Apply a `LISTENING_ROOM_UPDATE` payload. Malformed and stale updates are ignored. Returns the stored update. */
export function ingestRoomUpdate(raw: unknown): RoomUpdate | null {
  const u = parseRoomUpdate(raw);
  if (!u) return null;
  const prev = useListeningRooms.getState().rooms[u.channel_id];
  if (prev && !newer(u, prev)) return prev;
  useListeningRooms.setState((s) => ({ rooms: { ...s.rooms, [u.channel_id]: u } }));
  return u;
}

export function setListening(channelId: string | null): void {
  useListeningRooms.setState({ listening: channelId });
}

export function __resetListeningRooms(): void {
  useListeningRooms.setState({ rooms: {}, listening: null });
}
