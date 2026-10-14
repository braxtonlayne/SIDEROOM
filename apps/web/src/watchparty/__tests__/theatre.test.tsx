// The watch party page end to end in the app: the real router, the real HTTP layer (MSW stands in for the
// server), the real controller and the real Cinema player session over a fake HLS engine and a fake media
// element. The gateway is simulated with `emitDispatch`. No stream is played anywhere.
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { MiniPlayerMount } from '../../cinema/player/MiniPlayerMount';
import { installMediaMocks, makeSession, resetSession } from '../../cinema/player/__tests__/fakes';
import { createAppRouter } from '../../router';
import { emitDispatch } from '../../store/events';
import { useStore } from '../../store/store';
import { http, HttpResponse, server } from '../../test/server';
import { useVoiceSession } from '../../voice/session';
import { setPartyController } from '../runtime';
import type { Party, Playback } from '../types';
import { FakeServer, MEDIA, person } from './fakes';

// The page holds the gateway open while a party is followed; there is no socket in unit tests.
vi.mock('../../store/connect', async (orig) => ({
  ...(await orig<typeof import('../../store/connect')>()),
  holdGateway: () => () => undefined,
}));

beforeAll(() => {
  installMediaMocks();
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = false;
});

let fake: FakeServer;
const posted: { path: string; body: unknown }[] = [];

function api(o: { host?: boolean; control?: boolean; bound?: boolean } = {}) {
  fake = new FakeServer(() => Date.now(), '1', { host: o.host ?? true, control: o.control });
  if (o.bound) {
    fake.party = {
      ...fake.party,
      binding: 'channel',
      guild_id: '5',
      channel_id: '77',
      invite_code: null,
    };
  }
  posted.length = 0;
  const party = (): Party => ({ ...fake.party, playback: fake.view() });
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/watch-parties/time', () =>
      HttpResponse.json({ server_time_ms: Date.now() }),
    ),
    http.get('*/api/v1/watch-parties/:id', () => HttpResponse.json(party())),
    http.post('*/api/v1/watch-parties/:id/join', () => HttpResponse.json(party())),
    http.post('*/api/v1/watch-parties/:id/heartbeat', async ({ request }) => {
      posted.push({ path: 'heartbeat', body: await request.json() });
      return HttpResponse.json({ server_time_ms: Date.now(), playback: fake.view(), member: true });
    }),
    http.post('*/api/v1/watch-parties/:id/playback', async ({ request }) => {
      const body = (await request.json()) as { action: string; position_ms?: number };
      posted.push({ path: 'playback', body });
      if (body.action === 'play') fake.change({ playing: true });
      if (body.action === 'pause') fake.change({ playing: false });
      if (body.action === 'seek') fake.change({ position_ms: body.position_ms ?? 0 });
      return HttpResponse.json(fake.view());
    }),
    http.post('*/api/v1/watch-parties/:id/reactions', async ({ request }) => {
      posted.push({ path: 'reactions', body: await request.json() });
      return new HttpResponse(null, { status: 204 });
    }),
    http.post('*/api/v1/watch-parties/:id/messages', async ({ request }) => {
      posted.push({ path: 'messages', body: await request.json() });
      return new HttpResponse(null, { status: 204 });
    }),
    http.patch('*/api/v1/watch-parties/:id/queue/:item', async ({ request }) => {
      posted.push({ path: 'queue-move', body: await request.json() });
      return HttpResponse.json(party());
    }),
    http.patch('*/api/v1/watch-parties/:id', async ({ request }) => {
      const body = (await request.json()) as Partial<Party['settings']>;
      posted.push({ path: 'settings', body });
      fake.party = { ...fake.party, settings: { ...fake.party.settings, ...body } };
      return HttpResponse.json(party());
    }),
    // The Cinema side: the play target, the grant and the viewer's progress.
    http.get('*/api/v1/cinema/titles/:id/play', () =>
      HttpResponse.json({
        title_id: MEDIA.title_id,
        episode_id: null,
        variant_id: null,
        asset_id: MEDIA.asset_id,
        title: MEDIA.title,
        runtime_min: 120,
        age_restricted: false,
        markers: [],
      }),
    ),
    http.get('*/api/v1/cinema/assets/:id/playback', () =>
      HttpResponse.json({
        master_url: 'https://cdn.test/501/master.m3u8?sig=a',
        expires_at: Math.floor(Date.now() / 1000) + 6 * 3600,
        duration_secs: 7200,
        subtitles: [],
      }),
    ),
    http.get('*/api/v1/cinema/titles/:id/progress', () =>
      HttpResponse.json({ message: 'none' }, { status: 404 }),
    ),
    http.put('*/api/v1/cinema/titles/:id/progress', () => new HttpResponse(null, { status: 204 })),
  );
  return fake;
}

function renderParty(path = '/cinema/party/900?code=abc') {
  const ctx = makeSession();
  setPartyController(null); // a fresh controller bound to this session
  const history = createMemoryHistory({ initialEntries: [path] });
  const router = createAppRouter(history);
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
      <MiniPlayerMount history={router.history} />
    </QueryClientProvider>,
  );
  return { ...ctx, router, history, ...view, video: ctx.session.element() };
}

/** Deliver a gateway dispatch the way the store does. */
const gateway = async (event: string, data: unknown) => {
  emitDispatch(event, data);
  await Promise.resolve();
};
const playbackEvent = (reason: string, pb: Playback = fake.view()) =>
  gateway('WATCH_PARTY_PLAYBACK', { party_id: '900', reason, by: '2', playback: pb });
const updateEvent = (type: string, patch: Partial<Party> = {}) =>
  gateway('WATCH_PARTY_UPDATE', {
    type,
    party: { ...fake.party, ...patch, you: null, playback: fake.view() },
  });

beforeEach(() => {
  useStore.setState({ currentUserId: '1' });
});
afterEach(() => {
  setPartyController(null);
  resetSession();
  useVoiceSession.setState({ speaking: {} });
  useStore.setState({ voiceStates: {} });
});

describe('the theatre', () => {
  it('joins, loads the media and lets the host drive playback through the server; passes axe', async () => {
    api();
    const ctx = renderParty();
    expect(await screen.findByRole('heading', { name: 'Akira' })).toBeInTheDocument();
    expect(await screen.findByText('2 watching')).toBeInTheDocument();
    await waitFor(() => expect(ctx.engine.load).toHaveBeenCalled());

    const found = await axeViolations(ctx.container);
    expect(found.map((v) => JSON.stringify(v).slice(0, 400))).toEqual([]);

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Play for everyone' }));
    await waitFor(() => expect(posted.some((p) => p.path === 'playback')).toBe(true));
    expect(posted.find((p) => p.path === 'playback')?.body).toMatchObject({
      action: 'play',
      expected_version: 1,
    });
    // The answer is applied like any server state: the element plays.
    await waitFor(() => expect(ctx.video.paused).toBe(false));
    expect(await screen.findByRole('button', { name: 'Pause for everyone' })).toBeInTheDocument();
  });

  it('shows the controls disabled to a viewer, who can raise a hand instead', async () => {
    api({ host: false, control: false });
    renderParty();
    const play = await screen.findByRole('button', { name: 'Play for everyone' });
    expect(play).toHaveAttribute('aria-disabled', 'true');
    expect(screen.getByRole('slider', { name: 'Position' })).toBeDisabled();
    expect(screen.getByRole('combobox', { name: 'Speed' })).toBeDisabled();

    const user = userEvent.setup();
    await user.click(play);
    expect(posted.filter((p) => p.path === 'playback')).toEqual([]);

    await user.click(screen.getByRole('button', { name: 'Request control' }));
    await waitFor(() => expect(posted.find((p) => p.path === 'reactions')).toBeDefined());
    expect(posted.find((p) => p.path === 'reactions')?.body).toEqual({ emoji: '✋' });
    // No end-party or settings for a plain viewer.
    expect(screen.queryByRole('button', { name: 'End party' })).not.toBeInTheDocument();
  });

  it('follows the server: another controller plays, pauses and seeks', async () => {
    api({ host: false, control: false });
    const ctx = renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    await waitFor(() => expect(ctx.engine.load).toHaveBeenCalled());
    fake.change({ playing: true });
    await playbackEvent('play');
    await waitFor(() => expect(ctx.video.paused).toBe(false));
    expect(await screen.findByRole('button', { name: 'Pause for everyone' })).toBeInTheDocument();

    fake.change({ playing: false, position_ms: 90_000 });
    await playbackEvent('pause');
    await waitFor(() => expect(ctx.video.paused).toBe(true));
    expect(ctx.video.currentTime).toBe(90);
  });

  it('says who the party is waiting for', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    fake.party = {
      ...fake.party,
      participants: [
        person('1', { is_host: true }),
        person('2', { buffering: true, username: 'ana' }),
      ],
    };
    fake.change({ playing: false, auto_paused: true });
    await updateEvent('participant_updated');
    await playbackEvent('buffering');
    expect(await screen.findByText(/Waiting for ana/)).toBeInTheDocument();
  });

  it('lists the people with host and buffering marks and a speaking indicator from the voice call', async () => {
    api({ bound: true });
    fake.party = {
      ...fake.party,
      participants: [
        person('1', { is_host: true, username: 'me' }),
        person('2', { username: 'ana', buffering: true }),
        person('3', { username: 'bo', ready: true, is_controller: true }),
      ],
    };
    useStore.setState({
      voiceStates: {
        '5': {
          '2': { user_id: '2', channel_id: '77' },
          '3': { user_id: '3', channel_id: '77' },
        },
      },
    });
    useVoiceSession.setState({ speaking: { '2': true } });
    renderParty('/cinema/party/900');
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: 'People' }));
    const list = await screen.findByRole('list', { name: 'Participants' });
    const rows = within(list).getAllByRole('listitem');
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText('Host')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('Buffering')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('speaking')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('In voice')).toBeInTheDocument();
    expect(within(rows[2]!).getByText('Controller')).toBeInTheDocument();
    expect(within(rows[2]!).getByText('Ready')).toBeInTheDocument();
    expect(within(rows[2]!).queryByText('speaking')).not.toBeInTheDocument();
  });

  it('lets the host change who controls playback', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: 'People' }));
    await user.selectOptions(
      await screen.findByLabelText('Who can play, pause and seek'),
      'everyone',
    );
    await waitFor(() => expect(posted.find((p) => p.path === 'settings')).toBeDefined());
    expect(posted.find((p) => p.path === 'settings')?.body).toEqual({ shared_control: 'everyone' });
  });

  it('shows a raised hand to the host', async () => {
    api();
    fake.party = {
      ...fake.party,
      participants: [person('1', { is_host: true }), person('2', { username: 'ana' })],
    };
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    await gateway('WATCH_PARTY_REACTION', { party_id: '900', user_id: '2', emoji: '✋', at_ms: 1 });
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: 'People' }));
    expect(await screen.findByText(/wants control/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Let ana control playback' })).toBeInTheDocument();
  });

  it('reorders the queue with the move buttons', async () => {
    api();
    const q = (id: string, title: string, position: number) => ({
      ...MEDIA,
      id,
      title,
      position,
      added_by: '1',
      added_at_ms: 1,
    });
    fake.party = { ...fake.party, queue: [q('a', 'Perfect Blue', 0), q('b', 'Paprika', 1)] };
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: /Queue/ }));
    expect(await screen.findByRole('list', { name: 'Queue' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Move Paprika up' }));
    await waitFor(() => expect(posted.find((p) => p.path === 'queue-move')).toBeDefined());
    expect(posted.find((p) => p.path === 'queue-move')?.body).toEqual({ position: 0 });
    expect(screen.getByRole('button', { name: 'Move Perfect Blue up' })).toBeDisabled();
  });

  it('sends chat messages and shows the ones that arrive', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Message'), 'this score');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(posted.find((p) => p.path === 'messages')).toBeDefined());
    expect(posted.find((p) => p.path === 'messages')?.body).toEqual({ content: 'this score' });
    await gateway('WATCH_PARTY_MESSAGE', {
      party_id: '900',
      user_id: '2',
      content: 'so good',
      at_ms: 1,
    });
    expect(await screen.findByText('so good')).toBeInTheDocument();
  });

  it('floats reactions over the picture, hidden from assistive tech', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    await gateway('WATCH_PARTY_REACTION', {
      party_id: '900',
      user_id: '2',
      emoji: '\u{1f37f}',
      at_ms: 1,
    });
    const el = await screen.findByText('\u{1f37f}', { selector: '.wp-reaction' });
    expect(el.closest('[aria-hidden="true"]')).not.toBeNull();
  });

  it('offers the invite link and copies it', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Invite' }));
    const dialog = await screen.findByRole('dialog', { name: 'Invite people' });
    expect(within(dialog).getByLabelText('Party link')).toHaveValue(
      `${window.location.origin}/cinema/party/900?code=abc`,
    );
    await user.click(within(dialog).getByRole('button', { name: 'Copy link' }));
    expect(await navigator.clipboard.readText()).toBe(
      `${window.location.origin}/cinema/party/900?code=abc`,
    );
  });

  it('shows a ready check before anything played, counting who is ready', async () => {
    api();
    fake.party = {
      ...fake.party,
      participants: [person('1', { is_host: true, ready: true }), person('2')],
    };
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const check = await screen.findByRole('region', { name: 'Ready check' });
    expect(within(check).getByText('1 of 2 ready')).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(within(check).getByRole('button', { name: 'Not ready' }));
    await waitFor(() =>
      expect(
        posted.find(
          (p) => p.path === 'heartbeat' && (p.body as { ready?: boolean }).ready === false,
        ),
      ).toBeDefined(),
    );
    // Starting playback dismisses it.
    fake.change({ playing: true });
    await playbackEvent('play');
    await waitFor(() =>
      expect(screen.queryByRole('region', { name: 'Ready check' })).not.toBeInTheDocument(),
    );
  });
});

describe('leaving', () => {
  it('hands the picture to the mini player, which expands back into the party', async () => {
    api();
    const ctx = renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Play for everyone' }));
    await waitFor(() => expect(ctx.video.paused).toBe(false));
    await user.click(screen.getByRole('button', { name: /Leave the theatre/ }));
    const mini = await screen.findByRole('region', { name: /Mini player: Akira/ });
    expect(ctx.video.paused).toBe(false); // still playing, still following the party
    await user.click(within(mini).getByRole('button', { name: 'Expand' }));
    await waitFor(() => expect(ctx.history.location.pathname).toBe('/cinema/party/900'));
    expect(await screen.findByRole('button', { name: 'Pause for everyone' })).toBeInTheDocument();
  });

  it('tells the viewer when the host ends the party', async () => {
    api();
    renderParty();
    await screen.findByRole('heading', { name: 'Akira' });
    fake.party = { ...fake.party, ended: true };
    await updateEvent('ended', { ended: true });
    expect(await screen.findByText('This party has ended')).toBeInTheDocument();
  });

  it('explains an age-restricted party to someone who has not confirmed their age', async () => {
    api();
    server.use(
      http.get('*/api/v1/watch-parties/:id', () =>
        HttpResponse.json({ code: 900011, message: 'age' }, { status: 403 }),
      ),
    );
    renderParty();
    expect(
      await screen.findByText('This party is showing age-restricted content'),
    ).toBeInTheDocument();
  });
});
