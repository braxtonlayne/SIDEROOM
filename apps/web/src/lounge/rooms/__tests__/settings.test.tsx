import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { emitDispatch } from '../../../store/events';
import { ListeningRoomSettings } from '../settings/ListeningRoomSettings';
import { __resetListeningRooms } from '../store';
import { closePlayerLibrary, isPlayerLibraryOpen } from '../../../player/presence';

// MUS-JB-003 (wave 50, D-509 / D-510): the "24/7 listening room" card of the voice channel editor.

const playlist = (id: string, name: string) => ({
  id,
  owner_type: 'guild',
  owner_id: 'g1',
  name,
  description: '',
  item_count: 2,
  created_ms: 1,
  updated_ms: 1,
  can_edit: true,
});

const cursor = (over: Record<string, unknown> = {}) => ({
  channel_id: 'c1',
  guild_id: 'g1',
  mode: 'cursor',
  dj_id: null,
  seq: 1,
  at_ms: 1_000,
  position_ms: 0,
  now_playing: {
    item_id: 'i1',
    track_id: 't1',
    title: 'Blue Hour',
    artist: 'Ada',
    duration_ms: 60_000,
  },
  next: [],
  skipped: 0,
  playlist_id: 'p1',
  ...over,
});

interface Call {
  method: string;
  body: unknown;
}

function api(opts: { room: object | null; lists: object[] }) {
  const calls: Call[] = [];
  server.use(
    http.get('*/api/v1/guilds/g1/music/playlists', () => HttpResponse.json(opts.lists)),
    http.get('*/api/v1/channels/c1/listening-room', () =>
      opts.room
        ? HttpResponse.json(opts.room)
        : HttpResponse.json({ code: 'SR_NOT_FOUND', message: 'no' }, { status: 404 }),
    ),
    http.put('*/api/v1/channels/c1/listening-room', async ({ request }) => {
      calls.push({ method: 'PUT', body: await request.json() });
      return new HttpResponse(null, { status: 204 });
    }),
    http.delete('*/api/v1/channels/c1/listening-room', () => {
      calls.push({ method: 'DELETE', body: undefined });
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

const card = (canManage = true) =>
  render(<ListeningRoomSettings channelId="c1" guildId="g1" canManage={canManage} />);

beforeEach(() => __resetListeningRooms());

describe('the listening room card', () => {
  it('starts off, and picking a playlist then saving sends the playlist', async () => {
    const calls = api({
      room: null,
      lists: [playlist('p1', 'Night shift'), playlist('p2', 'Lofi')],
    });
    card();
    expect(await screen.findByTestId('room-status')).toHaveTextContent('Status: Off');
    const save = screen.getByRole('button', { name: 'Save listening room' });
    expect(save).toBeDisabled();
    await userEvent.click(screen.getByRole('switch'));
    expect(save).toBeDisabled();
    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByRole('option', { name: 'Lofi' }));
    await userEvent.click(save);
    await waitFor(() => expect(calls).toEqual([{ method: 'PUT', body: { playlist_id: 'p2' } }]));
    expect(await screen.findByText('Listening room saved.')).toBeInTheDocument();
  });

  it('reads the room that is on, shows what plays and switching off sends DELETE', async () => {
    const calls = api({ room: cursor(), lists: [playlist('p1', 'Night shift')] });
    card();
    const status = await screen.findByTestId('room-status');
    await waitFor(() => expect(status).toHaveTextContent('Playing Blue Hour from Night shift'));
    expect(screen.getByRole('switch')).toBeChecked();
    await userEvent.click(screen.getByRole('switch'));
    await userEvent.click(screen.getByRole('button', { name: 'Save listening room' }));
    await waitFor(() => expect(calls).toEqual([{ method: 'DELETE', body: undefined }]));
    expect(await screen.findByText('Listening room switched off.')).toBeInTheDocument();
    expect(screen.getByTestId('room-status')).toHaveTextContent('Status: Off');
  });

  it('with no server playlists it says so and the switch stays off', async () => {
    api({ room: null, lists: [] });
    card();
    expect(
      await screen.findByText('Create a server playlist in the music library first.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('switch')).toBeDisabled();
    expect(screen.getByRole('switch')).not.toBeChecked();
  });

  it('the empty-list hint opens the music library (D-535)', async () => {
    api({ room: null, lists: [] });
    closePlayerLibrary();
    card();
    await userEvent.click(await screen.findByRole('button', { name: 'Open the music library' }));
    expect(isPlayerLibraryOpen()).toBe(true);
    closePlayerLibrary();
  });

  it('without Manage Channels everything is disabled and the sentence says why', async () => {
    api({ room: cursor(), lists: [playlist('p1', 'Night shift')] });
    card(false);
    expect(
      await screen.findByText(
        'You need the Manage Channels permission to change the listening room.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole('switch')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save listening room' })).toBeDisabled();
  });

  it('follows LISTENING_ROOM_UPDATE while open, including a live DJ', async () => {
    api({ room: cursor(), lists: [playlist('p1', 'Night shift')] });
    card();
    const status = await screen.findByTestId('room-status');
    await waitFor(() => expect(status).toHaveTextContent('Playing Blue Hour'));
    act(() =>
      emitDispatch(
        'LISTENING_ROOM_UPDATE',
        cursor({ seq: 1, at_ms: 2_000, mode: 'stream', dj_id: 'u9' }),
      ),
    );
    await waitFor(() =>
      expect(status).toHaveTextContent('A DJ is live, the playlist resumes when they finish'),
    );
    act(() =>
      emitDispatch(
        'LISTENING_ROOM_UPDATE',
        cursor({
          seq: 2,
          at_ms: 3_000,
          now_playing: {
            item_id: 'i2',
            track_id: 't2',
            title: 'Second Wind',
            artist: 'Bo',
            duration_ms: 60_000,
          },
        }),
      ),
    );
    await waitFor(() => expect(status).toHaveTextContent('Playing Second Wind from Night shift'));
  });
});
