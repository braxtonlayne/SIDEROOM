import { useEffect, useState } from 'react';
import { Button } from '@sideroom/ui';
import { apiPost } from '../../api/client';
import { useT } from '../../i18n';
import { onDispatch } from '../../store/events';
import type { RoomUpdate } from './cursor';
import { followRoom, joinRoom, leaveRoom, type JoinDeps, type RoomPlayer } from './join';
import { ingestRoomClosed, ingestRoomUpdate, useListeningRooms } from './store';
import './rooms.css';

export const ROOM_EVENT = 'LISTENING_ROOM_UPDATE';

/**
 * The real player behind `RoomPlayer`. The player store is imported on first use, never with the lounge: it is
 * app code that must not weigh on (or be reachable from) anything but a signed-in page.
 */
export async function realPlayer(): Promise<RoomPlayer> {
  const p = await import('../../player/store');
  return {
    playTracks: (tracks, startSec) => {
      p.play(
        tracks.map((t) => ({
          id: t.id,
          title: t.title,
          artist: t.artist,
          src: t.src,
          durationSec: t.durationSec,
        })),
        0,
      );
      if (startSec > 0) p.seek(startSec);
    },
    seek: p.seek,
    stop: p.stopAndForget,
    current: () => {
      const s = p.usePlayerStore.getState();
      return { trackId: s.queue[s.index]?.id ?? null, positionSec: s.position };
    },
  };
}

const presign = (trackId: string) =>
  apiPost<{ url: string }>(`/music/tracks/${encodeURIComponent(trackId)}/presign`, {}).then(
    (r) => r.url,
  );

async function realDeps(): Promise<JoinDeps> {
  return { player: await realPlayer(), presign, now: () => Date.now() };
}

/** Keeps the store current from the gateway while mounted, and keeps a joined listener on the cursor. */
function useRoomFeed(channelId: string) {
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event !== ROOM_EVENT) return;
        const closed = ingestRoomClosed(data);
        if (closed !== null) {
          // The room was switched off: the card goes, and a listener stops playing it.
          if (closed === channelId && useListeningRooms.getState().listening === channelId)
            void realDeps().then((d) => leaveRoom(d));
          return;
        }
        const u = ingestRoomUpdate(data);
        if (!u || u.channel_id !== channelId) return;
        if (useListeningRooms.getState().listening !== channelId) return;
        void realDeps().then((d) => followRoom(u, d));
      }),
    [channelId],
  );
}

/** Pure view of one cursor. */
export function RoomCardView({
  room,
  listening,
  failed,
  onJoin,
  onLeave,
}: {
  room: RoomUpdate | undefined;
  listening: boolean;
  failed: boolean;
  onJoin: () => void;
  onLeave: () => void;
}) {
  const t = useT();
  const np = room?.now_playing ?? null;
  return (
    <section className="lng-room-card" aria-label={t('lounge.rooms.card')} data-testid="room-card">
      <p className="lng-room-desc">{t('lounge.rooms.description')}</p>
      {room?.mode === 'stream' ? (
        <p className="lng-room-note">{t('lounge.rooms.stream')}</p>
      ) : (
        <>
          <h4 className="lng-room-h">{t('lounge.rooms.nowPlaying')}</h4>
          {np ? (
            <p className="lng-room-now" data-testid="room-now">
              <strong>{np.title || t('lounge.rooms.untitled')}</strong>
              <span>{np.artist || t('lounge.rooms.unknownArtist')}</span>
            </p>
          ) : (
            <p className="lng-room-note">{t('lounge.rooms.nothingPlaying')}</p>
          )}
          {room && room.next.length > 0 ? (
            <>
              <h4 className="lng-room-h">{t('lounge.rooms.nextUp')}</h4>
              <ol className="lng-room-next" data-testid="room-next">
                {room.next.map((n, i) => (
                  <li key={`${n.item_id}-${i}`}>
                    {n.title || t('lounge.rooms.untitled')} <span>{n.artist}</span>
                  </li>
                ))}
              </ol>
            </>
          ) : null}
          {room && room.skipped > 0 ? (
            <p className="lng-room-note">{t('lounge.rooms.skipped', { n: room.skipped })}</p>
          ) : null}
        </>
      )}
      {failed ? (
        <p className="lng-room-note" role="alert">
          {t('lounge.rooms.joinFailed')}
        </p>
      ) : null}
      {listening ? (
        <>
          <p className="lng-room-note" role="status">
            {t('lounge.rooms.listening')}
          </p>
          <Button onClick={onLeave}>{t('lounge.rooms.leave')}</Button>
        </>
      ) : (
        <Button onClick={onJoin} disabled={!np}>
          {t('lounge.rooms.join')}
        </Button>
      )}
    </section>
  );
}

/** The card a listening room shows under its name in the channel list. */
export function ListeningRoomCard({ channelId }: { channelId: string }) {
  const room = useListeningRooms((s) => s.rooms[channelId]);
  const listening = useListeningRooms((s) => s.listening === channelId);
  const [failed, setFailed] = useState(false);
  useRoomFeed(channelId);
  if (!room) return null;
  return (
    <RoomCardView
      room={room}
      listening={listening}
      failed={failed}
      onJoin={() => {
        setFailed(false);
        void realDeps()
          .then((d) => joinRoom(room, d))
          .catch(() => setFailed(true));
      }}
      onLeave={() => void realDeps().then((d) => leaveRoom(d))}
    />
  );
}
