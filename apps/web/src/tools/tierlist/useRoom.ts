import { useEffect, useMemo, useSyncExternalStore } from 'react';
import { onDispatch } from '../../store/events';
import { httpRoomApi, type RoomApi, type ToolRoom, type ToolRoomEvent } from './roomApi';
import { RoomSync, type RoomView, type SyncStatus } from './sync';

export interface LiveRoom {
  sync: RoomSync;
  view: RoomView;
  status: SyncStatus;
  /** Set or remove keys; the view changes at once and the server hears about it in order. */
  edit: (set: Record<string, unknown>, title?: string) => void;
}

/** One room, live: seeded from `room`, kept current by `TOOL_ROOM_UPDATE`, edited through the version rule. */
export function useLiveRoom(room: ToolRoom, api: RoomApi = httpRoomApi): LiveRoom {
  const sync = useMemo(() => new RoomSync(room, api), [room.id, room.channel_id, api]); // eslint-disable-line react-hooks/exhaustive-deps
  const view = useSyncExternalStore(sync.subscribe, sync.getView);
  const status = useSyncExternalStore(sync.subscribe, sync.getStatus);
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event !== 'TOOL_ROOM_UPDATE') return;
        const ev = data as ToolRoomEvent;
        if (ev.id === sync.room.id) void sync.receive(ev);
      }),
    [sync],
  );
  // A gateway reconnect may have dropped updates: look again when it resumes.
  useEffect(() => {
    const onOnline = () => void sync.refetch();
    window.addEventListener('online', onOnline);
    return () => window.removeEventListener('online', onOnline);
  }, [sync]);
  return { sync, view, status, edit: (set, title) => void sync.edit(set, title) };
}
