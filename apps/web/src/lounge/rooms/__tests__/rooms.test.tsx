import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import {
  follow,
  newer,
  parseRoomUpdate,
  planJoin,
  type RoomTrack,
  type RoomUpdate,
} from '../cursor';
import { followRoom, joinRoom, leaveRoom, type JoinDeps } from '../join';
import { RoomCardView } from '../ListeningRoomCard';
import { __resetListeningRooms, ingestRoomUpdate, useListeningRooms } from '../store';

const track = (n: number, ms: number): RoomTrack => ({
  item_id: `i${n}`,
  track_id: `t${n}`,
  title: `Song ${n}`,
  artist: `Artist ${n}`,
  duration_ms: ms,
});

const room = (over: Partial<RoomUpdate> = {}): RoomUpdate => ({
  channel_id: 'c1',
  guild_id: 'g1',
  mode: 'cursor',
  seq: 4,
  at_ms: 1_000_000,
  position_ms: 10_000,
  now_playing: track(1, 60_000),
  next: [track(2, 30_000), track(3, 30_000), track(4, 30_000)],
  skipped: 0,
  ...over,
});

const wire = (u: RoomUpdate) => ({
  ...u,
  now_playing: u.now_playing,
});

beforeEach(() => __resetListeningRooms());

describe('parseRoomUpdate', () => {
  it('reads a server payload and drops malformed ones', () => {
    expect(parseRoomUpdate(wire(room()))?.now_playing?.title).toBe('Song 1');
    expect(parseRoomUpdate(null)).toBeNull();
    expect(parseRoomUpdate({ ...room(), seq: 'x' })).toBeNull();
    expect(parseRoomUpdate({ ...room(), now_playing: { track_id: 't' } })).toBeNull();
    const idle = parseRoomUpdate({ ...room(), now_playing: null, next: [] });
    expect(idle?.now_playing).toBeNull();
  });

  it('keeps at most three next tracks and ignores bad ones', () => {
    const u = parseRoomUpdate({
      ...room(),
      next: [track(2, 1), track(3, 1), track(4, 1), track(5, 1), { nope: 1 }],
    });
    expect(u?.next.map((t) => t.track_id)).toEqual(['t2', 't3', 't4']);
  });

  it('orders updates by seq, then by time', () => {
    expect(newer(room({ seq: 5 }), room({ seq: 4, at_ms: 9e9 }))).toBe(true);
    expect(newer(room({ at_ms: 2 }), room({ at_ms: 1 }))).toBe(true);
    expect(newer(room(), room())).toBe(false);
  });
});

describe('join at the cursor', () => {
  it('starts the current track where the cursor is, advanced by the time since the update', () => {
    const p = planJoin(room(), 1_005_000);
    expect(p.kind).toBe('play');
    if (p.kind !== 'play') return;
    expect(p.seekSec).toBe(15);
    expect(p.tracks.map((t) => t.track_id)).toEqual(['t1', 't2', 't3', 't4']);
    expect(p.endsInMs).toBe(45_000);
  });

  it('moves into the next track when the event is late', () => {
    const p = planJoin(room(), 1_000_000 + 50_000 + 5_000);
    if (p.kind !== 'play') throw new Error('expected play');
    expect(p.tracks[0]!.track_id).toBe('t2');
    expect(p.seekSec).toBe(5);
  });

  it('waits when the cursor ran past everything it knows, never guesses a loop', () => {
    expect(planJoin(room(), 1_000_000 + 10 * 60_000).kind).toBe('wait');
  });

  it('an idle room and a DJ stream do not start local playback', () => {
    expect(planJoin(room({ now_playing: null, next: [] }), 1_000_000).kind).toBe('idle');
    expect(planJoin(room({ mode: 'stream' }), 1_000_000).kind).toBe('stream');
  });

  it('a clock behind the server never seeks before the start', () => {
    const p = planJoin(room({ position_ms: 0 }), 900_000);
    if (p.kind !== 'play') throw new Error('expected play');
    expect(p.seekSec).toBe(0);
  });

  it('follows without re-seeking inside the tolerance, seeks beyond it, rejoins on a new track', () => {
    const u = room();
    expect(follow(u, 1_005_000, 't1', 16.5)).toEqual({ kind: 'none' });
    expect(follow(u, 1_005_000, 't1', 30)).toEqual({ kind: 'seek', seekSec: 15 });
    expect(follow(u, 1_005_000, 't9', 15).kind).toBe('rejoin');
  });

  function deps(now = 1_005_000) {
    const calls: unknown[][] = [];
    const player = {
      playTracks: (...a: unknown[]) => calls.push(['play', ...a]),
      seek: (...a: unknown[]) => calls.push(['seek', ...a]),
      stop: () => calls.push(['stop']),
      current: () => ({ trackId: 't1', positionSec: 15 }),
    };
    const presign = vi.fn((id: string) => Promise.resolve(`https://bucket.test/${id}`));
    const d: JoinDeps = { player, presign, now: () => now };
    return { d, calls, presign };
  }

  it('joining presigns the queue and plays the first track at the cursor', async () => {
    const { d, calls, presign } = deps();
    await expect(joinRoom(room(), d)).resolves.toBe('playing');
    expect(presign).toHaveBeenCalledTimes(4);
    const [kind, queue, start] = calls[0] as [string, { id: string; src: string }[], number];
    expect(kind).toBe('play');
    expect(start).toBe(15);
    expect(queue[0]).toMatchObject({ id: 't1', src: 'https://bucket.test/t1', durationSec: 60 });
    expect(useListeningRooms.getState().listening).toBe('c1');
  });

  it('a refused presign plays nothing and joins nothing', async () => {
    const { d, calls } = deps();
    d.presign = () => Promise.reject(new Error('403'));
    await expect(joinRoom(room(), d)).rejects.toThrow('403');
    expect(calls).toEqual([]);
    expect(useListeningRooms.getState().listening).toBeNull();
  });

  it('an idle room starts nothing', async () => {
    const { d, calls } = deps();
    await expect(joinRoom(room({ now_playing: null, next: [] }), d)).resolves.toBe('idle');
    expect(calls).toEqual([]);
  });

  it('a listener follows a seek, a track change, and leaves when a DJ takes over', async () => {
    const { d, calls } = deps();
    await joinRoom(room(), d);
    calls.length = 0;
    await followRoom(room({ at_ms: 1_005_000, position_ms: 15_500 }), d);
    expect(calls).toEqual([]);
    d.player.current = () => ({ trackId: 't1', positionSec: 40 });
    await followRoom(room(), d);
    expect(calls[0]).toEqual(['seek', 15]);
    calls.length = 0;
    await followRoom(room({ seq: 5, now_playing: track(2, 30_000), position_ms: 0, at_ms: 1_005_000 }), d);
    expect(calls[0]![0]).toBe('play');
    calls.length = 0;
    await followRoom(room({ seq: 6, mode: 'stream' }), d);
    expect(calls).toEqual([['stop']]);
    expect(useListeningRooms.getState().listening).toBeNull();
    leaveRoom(d);
  });
});

describe('the store', () => {
  it('keeps the newest update of a room and ignores stale and malformed ones', () => {
    ingestRoomUpdate(wire(room({ seq: 5 })));
    ingestRoomUpdate(wire(room({ seq: 4, at_ms: 9e9 })));
    ingestRoomUpdate({ junk: true });
    expect(useListeningRooms.getState().rooms.c1?.seq).toBe(5);
  });
});

describe('the channel card', () => {
  const noop = () => undefined;

  it('shows what the room is, now playing and the next three', async () => {
    const { container } = render(
      <RoomCardView room={room()} listening={false} failed={false} onJoin={noop} onLeave={noop} />,
    );
    expect(screen.getByText(/sends no audio/i)).toBeInTheDocument();
    expect(screen.getByTestId('room-now')).toHaveTextContent('Song 1');
    const next = screen.getByTestId('room-next');
    expect(next.querySelectorAll('li')).toHaveLength(3);
    expect(next).toHaveTextContent('Song 4');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('joins and leaves from the buttons', async () => {
    const join = vi.fn();
    const leave = vi.fn();
    const { rerender } = render(
      <RoomCardView room={room()} listening={false} failed={false} onJoin={join} onLeave={leave} />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Listen along' }));
    expect(join).toHaveBeenCalledTimes(1);
    rerender(
      <RoomCardView room={room()} listening failed={false} onJoin={join} onLeave={leave} />,
    );
    await act(async () => userEvent.click(screen.getByRole('button', { name: 'Stop listening' })));
    expect(leave).toHaveBeenCalledTimes(1);
  });

  it('says so when a DJ streams, when nothing plays and how many tracks were left out', () => {
    const { rerender } = render(
      <RoomCardView room={room({ mode: 'stream' })} listening={false} failed={false} onJoin={noop} onLeave={noop} />,
    );
    expect(screen.getByText(/DJ is playing live/)).toBeInTheDocument();
    rerender(
      <RoomCardView
        room={room({ now_playing: null, next: [], skipped: 2 })}
        listening={false}
        failed
        onJoin={noop}
        onLeave={noop}
      />,
    );
    expect(screen.getByText('Nothing is playing yet.')).toBeInTheDocument();
    expect(screen.getByText(/2 tracks are left out/)).toBeInTheDocument();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Listen along' })).toBeDisabled();
  });
});
