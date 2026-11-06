import { Button, Select, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../../api/client';
import { useT } from '../../../i18n';
import { listGuildPlaylists, type PlaylistView } from '../../../music/playlists/api';
import { ActionStatus, useAction } from '../../../settings/server/ui';
import { useStore } from '../../../store';
import { onDispatch } from '../../../store/events';
import { memberKey } from '../../../store/reducer';
import type { RoomUpdate } from '../cursor';
import { ingestRoomClosed, ingestRoomUpdate, useListeningRooms } from '../store';

const enc = encodeURIComponent;
const ROOM_EVENT = 'LISTENING_ROOM_UPDATE';
const roomPath = (channelId: string) => `/channels/${enc(channelId)}/listening-room`;

/** What the server says about the room: `update` is null when the channel is not a room. */
interface RoomRead {
  update: RoomUpdate | null;
  playlistId: string | null;
}

async function readRoom(channelId: string): Promise<RoomRead> {
  try {
    const raw = await apiFetch<Record<string, unknown>>(roomPath(channelId), { retries: 0 });
    return {
      update: ingestRoomUpdate(raw) ?? useListeningRooms.getState().rooms[channelId] ?? null,
      playlistId: typeof raw.playlist_id === 'string' ? raw.playlist_id : null,
    };
  } catch (e) {
    // 404: the channel is not a room.
    if (e instanceof ApiError && e.status === 404) return { update: null, playlistId: null };
    throw e;
  }
}

/** The status line (D-509): Off, Playing a title from a playlist, or a DJ is live. */
type T = ReturnType<typeof useT>;

export function roomStatus(
  t: T,
  room: RoomUpdate | null,
  playlistName: string,
  djName: string | null,
): string {
  if (!room) return t('settings.rooms.statusOff');
  if (room.mode === 'stream')
    return djName
      ? t('settings.rooms.statusLive', { name: djName })
      : t('settings.rooms.statusLiveUnknown');
  const np = room.now_playing;
  if (!np) return t('settings.rooms.statusWaiting');
  return t('settings.rooms.statusPlaying', {
    title: np.title || t('lounge.rooms.untitled'),
    playlist: playlistName,
  });
}

type Loaded = { kind: 'ready'; read: RoomRead; lists: PlaylistView[] };

/**
 * The "24/7 listening room" card of a voice channel's editor (MUS-JB-003, D-509). `canManage` mirrors what the
 * server demands (`MANAGE_CHANNELS` on the channel); the server is the boundary, this only explains.
 */
export function ListeningRoomSettings({
  channelId,
  guildId,
  canManage,
}: {
  channelId: string;
  guildId: string;
  canManage: boolean;
}) {
  const t = useT();
  const action = useAction();
  const [state, setState] = useState<{ kind: 'loading' } | { kind: 'failed' } | Loaded>({
    kind: 'loading',
  });
  const [on, setOn] = useState(false);
  const [playlist, setPlaylist] = useState('');
  const live = useListeningRooms((s) => s.rooms[channelId] ?? null);
  const djId = live?.djId ?? null;
  const djName = useStore((s) =>
    djId
      ? (s.members[memberKey(guildId, djId)]?.nick ??
        (s.users[djId]?.global_name || s.users[djId]?.username) ??
        null)
      : null,
  );

  useEffect(() => {
    let alive = true;
    void Promise.all([readRoom(channelId), listGuildPlaylists(guildId)])
      .then(([read, lists]) => {
        if (!alive) return;
        setState({ kind: 'ready', read, lists });
        setOn(read.update !== null);
        setPlaylist(read.playlistId ?? '');
      })
      .catch(() => alive && setState({ kind: 'failed' }));
    return () => {
      alive = false;
    };
  }, [channelId, guildId]);

  // While open, the status follows the gateway (a closed room clears the card's state).
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event !== ROOM_EVENT) return;
        if (ingestRoomClosed(data) === null) ingestRoomUpdate(data);
      }),
    [],
  );

  if (state.kind === 'loading')
    return (
      <p className="ss-na" role="status">
        {t('settings.rooms.loading')}
      </p>
    );
  if (state.kind === 'failed')
    return (
      <p className="ss-na" role="alert">
        {t('settings.rooms.loadFailed')}
      </p>
    );

  const { read, lists } = state;
  const isRoom = read.update !== null;
  const noLists = lists.length === 0;
  const changed = on !== isRoom || (on && playlist !== (read.playlistId ?? ''));
  const canSave = canManage && changed && (!on || playlist !== '');
  const savedName = lists.find((l) => l.id === (read.playlistId ?? ''))?.name ?? '';

  const save = async () => {
    const turningOff = !on;
    const ok = await action.run(
      async () => {
        await apiFetch(roomPath(channelId), {
          retries: 0,
          method: turningOff ? 'DELETE' : 'PUT',
          ...(turningOff ? {} : { json: { playlist_id: playlist } }),
        });
        return true;
      },
      turningOff ? t('settings.rooms.savedOff') : t('settings.rooms.saved'),
    );
    if (!ok) return;
    const fresh: RoomRead = turningOff
      ? { update: null, playlistId: null }
      : await readRoom(channelId).catch(() => ({ update: null, playlistId: playlist }));
    setState({
      kind: 'ready',
      lists,
      read: { update: fresh.update, playlistId: turningOff ? null : playlist },
    });
  };

  return (
    <section className="ss-form" aria-labelledby="ss-room-title" data-testid="room-settings">
      <h3 className="ss-sub-title" id="ss-room-title">
        {t('settings.rooms.title')}
      </h3>
      <ActionStatus error={action.error} done={action.done} />
      {!canManage ? <p className="ss-na">{t('settings.rooms.noPermission')}</p> : null}
      <Switch
        label={t('settings.rooms.switch')}
        description={t('settings.rooms.hint')}
        checked={on}
        disabled={!canManage || action.busy || (noLists && !on)}
        onChange={(e) => setOn(e.target.checked)}
      />
      {noLists ? (
        <p className="ss-na">{t('settings.rooms.noPlaylists')}</p>
      ) : (
        <div>
          <label className="ss-field-label" htmlFor="ss-room-playlist">
            {t('settings.rooms.playlist')}
          </label>
          <Select
            id="ss-room-playlist"
            value={playlist || null}
            placeholder={t('settings.rooms.playlistPlaceholder')}
            disabled={!canManage || action.busy || !on}
            options={lists.map((l) => ({ value: l.id, label: l.name }))}
            onValueChange={setPlaylist}
          />
        </div>
      )}
      <p className="ss-na" role="status" data-testid="room-status">
        <strong>{t('settings.rooms.statusLabel')}: </strong>
        {roomStatus(t, isRoom ? (live ?? read.update) : null, savedName, djName)}
      </p>
      <div className="ss-actions">
        <Button loading={action.busy} disabled={!canSave} onClick={() => void save()}>
          {t('settings.rooms.save')}
        </Button>
      </div>
    </section>
  );
}
