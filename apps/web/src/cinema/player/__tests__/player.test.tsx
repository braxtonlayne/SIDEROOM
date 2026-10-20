import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import { MiniPlayerMount } from '../MiniPlayerMount';
import { setProgressApi } from '../progress';
import type { PlaybackInfo } from '../types';
import { LEVELS, installMediaMocks, makeInfo, makeSession, media, resetSession } from './fakes';

beforeAll(() => {
  installMediaMocks();
  // The session store updates from media events, outside React's event system.
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = false;
});
afterEach(() => {
  resetSession();
  setProgressApi(null);
});

interface Opts {
  info?: Partial<PlaybackInfo>;
  /** Called with the request URL; return a Response to override. */
  playback?: (url: URL) => Response | undefined;
}

function useCinemaApi(o: Opts = {}) {
  const requests: string[] = [];
  const info = makeInfo(o.info);
  const epId = info.episode_id;
  const target = (titleId: string) => ({
    title_id: titleId,
    episode_id: epId,
    variant_id: null,
    asset_id: info.media_id,
    title: info.title,
    episode_title: info.episode_label ? info.episode_label.split(' · ').slice(2).join(' · ') : null,
    season_number: epId ? 1 : null,
    episode_number: epId ? 1 : null,
    runtime_min: Math.round(info.duration_s / 60),
    age_restricted: info.age_restricted,
    markers: info.markers.map((m) => ({
      kind: m.kind,
      start_ms: m.start_s * 1000,
      end_ms: m.end_s * 1000,
    })),
    next_episode_id: info.next?.episode_id ?? null,
  });
  const wireProgress = (id: string, ep: string) =>
    info.progress
      ? HttpResponse.json({
          title_id: id,
          episode_id: ep,
          position_secs: info.progress.position_s,
          duration_secs: info.progress.duration_s,
          percent: (info.progress.position_s / info.progress.duration_s) * 100,
          completed: info.progress.completed,
          updated_at_ms: 1,
        })
      : HttpResponse.json({ message: 'none' }, { status: 404 });
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/cinema/titles/:id/play', ({ request, params }) => {
      const url = new URL(request.url);
      requests.push(`title:${String(params.id)}${url.search}`);
      return o.playback?.(url) ?? HttpResponse.json(target(String(params.id)));
    }),
    http.get('*/api/v1/cinema/episodes/:id/play', ({ request, params }) => {
      requests.push(`episode:${String(params.id)}`);
      return o.playback?.(new URL(request.url)) ?? HttpResponse.json(target(info.title_id));
    }),
    http.get('*/api/v1/cinema/episodes/:id', ({ params }) =>
      HttpResponse.json({
        id: String(params.id),
        title_id: info.next?.title_id ?? info.title_id,
        season_number: info.next?.season ?? 1,
        number: info.next?.number ?? 2,
        title: info.next?.title ?? 'Next',
        still_url: info.next?.still_url ?? null,
      }),
    ),
    http.get('*/api/v1/cinema/assets/:id/playback', () =>
      HttpResponse.json({
        master_url: info.master_url,
        expires_at: Math.floor(info.expires_at_ms / 1000),
        duration_secs: info.duration_s,
        sprites_url: info.sprites_url,
        subtitles: info.subtitle_tracks.map((s) => ({
          id: s.id,
          language: s.language,
          label: s.label,
          vtt_url: s.vtt_url,
          ass_url: s.ass_url,
        })),
      }),
    ),
    http.get('*/api/v1/cinema/titles/:id/progress', ({ params }) =>
      wireProgress(String(params.id), '0'),
    ),
    http.get('*/api/v1/cinema/titles/:id/episodes/:ep/progress', ({ params }) =>
      wireProgress(String(params.id), String(params.ep)),
    ),
    http.put('*/api/v1/cinema/titles/:id/progress', () => new HttpResponse(null, { status: 204 })),
    http.put(
      '*/api/v1/cinema/titles/:id/episodes/:ep/progress',
      () => new HttpResponse(null, { status: 204 }),
    ),
  );
  return requests;
}

function renderApp(path: string) {
  const ctx = makeSession();
  const history = createMemoryHistory({ initialEntries: [path] });
  const router = createAppRouter(history);
  const client = new QueryClient();
  const view = render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
      <MiniPlayerMount history={router.history} />
    </QueryClientProvider>,
  );
  return { ...ctx, router, history, ...view, video: ctx.session.element() };
}

const player = () => screen.findByRole('region', { name: /Video player/ });

async function openAndPlay(path = '/cinema/watch/t1') {
  const ctx = renderApp(path);
  const user = userEvent.setup();
  await player();
  await user.click(await screen.findByRole('button', { name: 'Play' }));
  await waitFor(() => expect(ctx.session.state.playing).toBe(true));
  return { ...ctx, user };
}

describe('watch page', () => {
  it('opens the title, asks for a gesture instead of autoplaying, then plays; passes axe', async () => {
    useCinemaApi({
      info: { kind: 'series', episode_id: 'e1', episode_label: 'S1 · E1 · Pilot' },
    });
    const ctx = renderApp('/cinema/watch/t1?episode=e1');
    expect(await screen.findByRole('heading', { name: 'Akira' })).toBeInTheDocument();
    expect(screen.getByText('S1 · E1 · Pilot')).toBeInTheDocument();
    // No sound without a gesture: the big Play button is there and nothing is playing.
    const big = await screen.findByRole('button', { name: 'Play' });
    expect(ctx.session.state.playing).toBe(false);
    expect(ctx.engine.load).toHaveBeenCalledWith(expect.stringContaining('master.m3u8'), undefined);
    const found = await axeViolations(ctx.container);
    expect(found.map((v) => JSON.stringify(v).slice(0, 400))).toEqual([]);
    await userEvent.setup().click(big);
    await waitFor(() => expect(ctx.session.state.playing).toBe(true));
    expect(screen.queryByRole('button', { name: 'Play' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pause' })).toBeInTheDocument();
  });

  it('requests the episode and the start time from the link', async () => {
    const requests = useCinemaApi();
    const ctx = renderApp('/cinema/watch/t1?episode=e5&t=120');
    await player();
    expect(requests).toEqual(['episode:e5']);
    await waitFor(() => expect(ctx.engine.load).toHaveBeenCalledWith(expect.any(String), 120));
    // An explicit start time replaces the resume prompt.
    expect(screen.queryByRole('group', { name: 'Resume playback' })).not.toBeInTheDocument();
  });

  it('says a title was removed after a notice (MOD-TKD-011)', async () => {
    useCinemaApi({
      playback: () => HttpResponse.json({ message: 'Not found' }, { status: 404 }),
    });
    server.use(
      http.get('*/api/v1/moderation/item-status', () => HttpResponse.json({ state: 'taken_down' })),
    );
    renderApp('/cinema/watch/nope');
    expect(await screen.findByText('Removed after a notice')).toBeInTheDocument();
  });

  it('shows a banner while the title is held', async () => {
    useCinemaApi();
    server.use(
      http.get('*/api/v1/moderation/item-status', () => HttpResponse.json({ state: 'held' })),
    );
    renderApp('/cinema/watch/t1');
    await player();
    expect(await screen.findByText('Under review')).toBeInTheDocument();
  });

  it('shows an error page for a missing title', async () => {
    useCinemaApi({
      playback: () => HttpResponse.json({ message: 'Not found' }, { status: 404 }),
    });
    renderApp('/cinema/watch/nope');
    expect(await screen.findByText('This title is not available')).toBeInTheDocument();
  });
});

describe('resume prompt (CIN-PLY-008)', () => {
  const progress = { position_s: 2530, duration_s: 6000, completed: false };

  it('offers the saved position and resumes from it', async () => {
    useCinemaApi({ info: { progress } });
    const ctx = renderApp('/cinema/watch/t1');
    const user = userEvent.setup();
    const group = await screen.findByRole('group', { name: 'Resume playback' });
    await user.click(within(group).getByRole('button', { name: 'Resume from 42:10' }));
    await waitFor(() => expect(ctx.session.state.playing).toBe(true));
    expect(media.time(ctx.video)).toBe(2530);
    expect(screen.queryByRole('group', { name: 'Resume playback' })).not.toBeInTheDocument();
  });

  it('starts over from zero', async () => {
    useCinemaApi({ info: { progress } });
    const ctx = renderApp('/cinema/watch/t1');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Start over' }));
    await waitFor(() => expect(ctx.session.state.playing).toBe(true));
    expect(media.time(ctx.video)).toBe(0);
  });

  it('does not offer it for a finished item', async () => {
    useCinemaApi({ info: { progress: { ...progress, completed: true } } });
    renderApp('/cinema/watch/t1');
    await player();
    expect(screen.queryByRole('group', { name: 'Resume playback' })).not.toBeInTheDocument();
  });
});

describe('keyboard shortcuts (CIN-PLY-003)', () => {
  it('space/k toggle, j/l and arrows seek, with on-screen feedback', async () => {
    useCinemaApi();
    const { user, video } = await openAndPlay();
    media.setDuration(video, 6000);
    media.tick(video, 100);
    await user.keyboard('l');
    expect(media.time(video)).toBe(110);
    expect(await screen.findAllByText('+10 s')).not.toHaveLength(0); // on screen and in the live region
    await user.keyboard('j');
    expect(media.time(video)).toBe(100);
    await user.keyboard('{ArrowRight}');
    expect(media.time(video)).toBe(105);
    await user.keyboard('{ArrowLeft}');
    expect(media.time(video)).toBe(100);
    await user.keyboard('k');
    expect(media.isPaused(video)).toBe(true);
    await user.keyboard(' ');
    expect(media.isPaused(video)).toBe(false);
    await user.keyboard('5');
    expect(media.time(video)).toBe(3000);
  });

  it('volume, mute, speed and captions', async () => {
    useCinemaApi();
    const { user, video, session } = await openAndPlay();
    await user.keyboard('{ArrowDown}');
    expect(session.state.volume).toBeCloseTo(0.95);
    await user.keyboard('m');
    expect(session.state.muted).toBe(true);
    expect(await screen.findAllByText('Muted')).not.toHaveLength(0);
    await user.keyboard('>');
    expect(media.rate(video)).toBe(1.25);
    await user.keyboard('<');
    expect(media.rate(video)).toBe(1);
    await user.keyboard('c');
    await waitFor(() => expect(video.querySelectorAll('track')).toHaveLength(1));
    await user.keyboard('c');
    await waitFor(() => expect(video.querySelectorAll('track')).toHaveLength(0));
  });

  it('leaves space to a focused button instead of toggling twice', async () => {
    useCinemaApi();
    const { user, video } = await openAndPlay();
    screen.getByRole('button', { name: 'Forward 10 seconds' }).focus();
    media.setDuration(video, 6000);
    await user.keyboard(' ');
    expect(media.time(video)).toBe(10); // the button's own click, not a play toggle
    expect(media.isPaused(video)).toBe(false);
  });

  it('does not steal keys typed into a field', async () => {
    useCinemaApi();
    const { user, video } = await openAndPlay();
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    await user.keyboard('k');
    expect(media.isPaused(video)).toBe(false);
    input.remove();
  });
});

describe('menus and controls (CIN-PLY-002)', () => {
  it('switches subtitles and their style from the menu', async () => {
    useCinemaApi();
    const { user, video, session } = await openAndPlay();
    await user.click(screen.getByRole('button', { name: 'Subtitles' }));
    const menu = await screen.findByRole('menu', { name: 'Subtitles' });
    await user.click(within(menu).getByRole('menuitemradio', { name: 'Japanese' }));
    await waitFor(() => expect(video.querySelector('track')).toHaveAttribute('srclang', 'ja'));
    await user.click(screen.getByRole('button', { name: 'Subtitles' }));
    await user.click(await screen.findByRole('menuitemradio', { name: 'Large' }));
    expect(session.state.subtitleStyle.size).toBe('large');
    expect(
      screen.getByRole('region', { name: /Video player/ }).style.getPropertyValue('--sr-cue-size'),
    ).toBe('1.35em');
  });

  it('offers quality and data saver once the stream reports its ladder', async () => {
    useCinemaApi();
    const { user, engine, session } = await openAndPlay();
    expect(screen.queryByRole('button', { name: 'Quality' })).not.toBeInTheDocument();
    act(() => engine.emit({ type: 'levels', levels: LEVELS }));
    await user.click(await screen.findByRole('button', { name: 'Quality' }));
    await user.click(await screen.findByRole('menuitemradio', { name: '720p' }));
    expect(engine.setQuality).toHaveBeenCalledWith(1);
    await user.click(screen.getByRole('button', { name: 'Quality' }));
    await user.click(await screen.findByRole('menuitemcheckbox', { name: /Data saver/ }));
    expect(engine.setMaxHeight).toHaveBeenLastCalledWith(480);
    expect(session.state.dataSaver).toBe(true);
  });

  it('shows the audio menu only with more than one track', async () => {
    useCinemaApi();
    const { user, engine } = await openAndPlay();
    expect(screen.queryByRole('button', { name: 'Audio track' })).not.toBeInTheDocument();
    act(() =>
      engine.emit({
        type: 'audio',
        tracks: [
          { id: '0', label: 'English', language: 'en' },
          { id: '1', label: 'Japanese', language: 'ja' },
        ],
        active: '0',
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Audio track' }));
    await user.click(await screen.findByRole('menuitemradio', { name: 'Japanese' }));
    expect(engine.setAudioTrack).toHaveBeenCalledWith('1');
  });

  it('seeks from the scrubber and exposes the time to screen readers', async () => {
    useCinemaApi();
    const { video } = await openAndPlay();
    media.setDuration(video, 6000);
    act(() => media.tick(video, 65));
    const seek = screen.getByRole('slider', { name: 'Seek' });
    expect(seek).toHaveAttribute('aria-valuetext', '1 minute 5 seconds of 1 hour 40 minutes');
    fireEvent.change(seek, { target: { value: '300' } });
    expect(media.time(video)).toBe(300);
  });

  it('shows sprite thumbnails when hovering the scrubber', async () => {
    server.use(
      http.get('https://cdn.test/m1/sprites.vtt', () =>
        HttpResponse.text(
          'WEBVTT\n\n00:00:00.000 --> 00:01:00.000\nsheet.jpg#xywh=0,0,160,90\n\n00:01:00.000 --> 00:02:00.000\nsheet.jpg#xywh=160,0,160,90\n',
        ),
      ),
    );
    useCinemaApi({ info: { sprites_url: 'https://cdn.test/m1/sprites.vtt' } });
    const { video } = await openAndPlay();
    media.setDuration(video, 120);
    const seek = screen.getByRole('slider', { name: 'Seek' });
    const wrap = seek.parentElement!;
    wrap.getBoundingClientRect = () =>
      ({
        left: 0,
        width: 1000,
        top: 0,
        height: 20,
        right: 1000,
        bottom: 20,
        x: 0,
        y: 0,
      }) as DOMRect;
    // Wait for the sprite map before hovering.
    await new Promise((r) => setTimeout(r, 50));
    // jsdom has no PointerEvent: a plain event of the right type carries the position.
    act(() => {
      wrap.dispatchEvent(new MouseEvent('pointermove', { clientX: 750, bubbles: true }));
    });
    const thumb = await screen.findByTestId('scrub-thumb');
    expect(thumb.style.backgroundPosition).toBe('-160px 0px');
  });
});

describe('skip intro and credits (CIN-PLY-005)', () => {
  it('shows the button only inside a marker and seeks to its end', async () => {
    useCinemaApi({
      info: {
        markers: [
          { kind: 'intro', start_s: 30, end_s: 90 },
          { kind: 'credits', start_s: 5000, end_s: 5400 },
        ],
      },
    });
    const { video, user } = await openAndPlay();
    media.setDuration(video, 6000);
    act(() => media.tick(video, 10));
    expect(screen.queryByRole('button', { name: 'Skip intro' })).not.toBeInTheDocument();
    act(() => media.tick(video, 45));
    await user.click(await screen.findByRole('button', { name: 'Skip intro' }));
    expect(media.time(video)).toBe(90);
    act(() => media.tick(video, 90));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Skip intro' })).not.toBeInTheDocument(),
    );
    act(() => media.tick(video, 5100));
    expect(await screen.findByRole('button', { name: 'Skip credits' })).toBeInTheDocument();
  });
});

describe('next episode (CIN-PLY-006)', () => {
  const next = { title_id: 't1', episode_id: 'e2', season: 1, number: 2, title: 'The Second' };
  const series = {
    kind: 'series' as const,
    episode_id: 'e1',
    episode_label: 'S1 · E1 · Pilot',
    duration_s: 1500,
    markers: [{ kind: 'credits' as const, start_s: 1400, end_s: 1500 }],
    next,
  };

  it('shows the card at the credits with a countdown and goes to the next episode', async () => {
    useCinemaApi({ info: series });
    const ctx = await openAndPlay('/cinema/watch/t1?episode=e1');
    media.setDuration(ctx.video, 1500);
    expect(screen.queryByRole('region', { name: 'Next episode' })).not.toBeInTheDocument();
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    try {
      act(() => media.tick(ctx.video, 1401));
      const card = screen.getByRole('region', { name: 'Next episode' });
      expect(within(card).getByText('S1 · E2 · The Second')).toBeInTheDocument();
      expect(within(card).getByText('Starts in 10 s')).toBeInTheDocument();
      await act(async () => {
        vi.advanceTimersByTime(3000);
      });
      expect(within(card).getByText('Starts in 7 s')).toBeInTheDocument();
      await act(async () => {
        vi.advanceTimersByTime(7000);
        await Promise.resolve();
      });
    } finally {
      vi.useRealTimers();
    }
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/watch/t1'));
    await waitFor(() => expect(ctx.router.state.location.search).toMatchObject({ episode: 'e2' }));
  });

  it('cancel keeps watching, Play now goes straight away', async () => {
    useCinemaApi({ info: series });
    const ctx = await openAndPlay('/cinema/watch/t1?episode=e1');
    media.setDuration(ctx.video, 1500);
    act(() => media.tick(ctx.video, 1450));
    await ctx.user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('region', { name: 'Next episode' })).not.toBeInTheDocument();
    expect(ctx.router.state.location.search).toMatchObject({ episode: 'e1' });
  });

  it('without autoplay the card waits for the viewer', async () => {
    useCinemaApi({ info: series });
    const ctx = await openAndPlay('/cinema/watch/t1?episode=e1');
    ctx.session.setAutoplayNext(false);
    media.setDuration(ctx.video, 1500);
    act(() => media.tick(ctx.video, 1450));
    const card = await screen.findByRole('region', { name: 'Next episode' });
    expect(within(card).queryByText(/Starts in/)).not.toBeInTheDocument();
    await ctx.user.click(within(card).getByRole('button', { name: 'Play now' }));
    await waitFor(() => expect(ctx.router.state.location.search).toMatchObject({ episode: 'e2' }));
  });

  it('asks "Are you still watching?" after three auto-played episodes instead of counting down', async () => {
    useCinemaApi({ info: series });
    const ctx = await openAndPlay('/cinema/watch/t1?episode=e1');
    act(() => ctx.session.store.setState({ autoplayedCount: 3 }));
    media.setDuration(ctx.video, 1500);
    act(() => media.tick(ctx.video, 1450));
    const dialog = await screen.findByRole('dialog', { name: 'Are you still watching?' });
    expect(media.isPaused(ctx.video)).toBe(true);
    expect(screen.queryByRole('region', { name: 'Next episode' })).not.toBeInTheDocument();
    await ctx.user.click(within(dialog).getByRole('button', { name: 'Keep watching' }));
    expect(ctx.session.state.autoplayedCount).toBe(0);
    expect(await screen.findByRole('region', { name: 'Next episode' })).toBeInTheDocument();
    expect(media.isPaused(ctx.video)).toBe(false);
  });

  it('the autoplay switch is remembered', async () => {
    useCinemaApi({ info: series });
    const ctx = await openAndPlay('/cinema/watch/t1?episode=e1');
    const box = screen.getByRole('checkbox', { name: 'Autoplay next episode' });
    expect(box).toBeChecked();
    await ctx.user.click(box);
    expect(ctx.session.state.autoplayNext).toBe(false);
    expect(JSON.parse(localStorage.getItem('sr.cinema.player.v1')!).autoplayNext).toBe(false);
  });
});

describe('age gate (D-184)', () => {
  const gated = (url: URL) => {
    void url;
    return HttpResponse.json(
      { code: 900011, message: 'Age confirmation required' },
      { status: 403 },
    );
  };

  it('asks, and on yes loads the title', async () => {
    let confirmed = false;
    useCinemaApi({ playback: (u) => (confirmed ? undefined : gated(u)) });
    server.use(
      http.post('*/api/v1/sr/age-gate', async ({ request }) => {
        expect(await request.json()).toEqual({ over_18: true });
        confirmed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const ctx = renderApp('/cinema/watch/t1');
    const dialog = await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await userEvent.setup().click(within(dialog).getByRole('button', { name: 'Yes' }));
    expect(await screen.findByRole('heading', { name: 'Akira' })).toBeInTheDocument();
    expect(ctx.engine.load).toHaveBeenCalled();
  });

  it('on no the player stays closed and the viewer goes back', async () => {
    useCinemaApi({ playback: gated });
    server.use(http.post('*/api/v1/sr/age-gate', () => new HttpResponse(null, { status: 204 })));
    const history = createMemoryHistory({ initialEntries: ['/support', '/cinema/watch/t1'] });
    makeSession();
    const router = createAppRouter(history);
    render(
      <QueryClientProvider client={new QueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    const dialog = await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await userEvent.setup().click(within(dialog).getByRole('button', { name: 'No' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/support'));
    expect(screen.queryByRole('heading', { name: 'Akira' })).not.toBeInTheDocument();
  });
});

describe('mini player (CIN-PLY-011)', () => {
  it('keeps playing when the viewer navigates away and expands back without reloading', async () => {
    const requests = useCinemaApi();
    const ctx = await openAndPlay();
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
    act(() => ctx.history.push('/support'));
    const mini = await screen.findByRole('region', { name: 'Mini player: Akira' });
    expect(mini.querySelector('video')).toBe(ctx.video);
    expect(media.isPaused(ctx.video)).toBe(false);
    expect(ctx.session.state.mode).toBe('mini');
    expect(ctx.engine.destroyed).toBe(false);

    await ctx.user.click(within(mini).getByRole('button', { name: 'Pause' }));
    expect(media.isPaused(ctx.video)).toBe(true);
    await ctx.user.click(within(mini).getByRole('button', { name: 'Expand' }));
    expect(await screen.findByRole('region', { name: /Video player/ })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
    expect(requests).toHaveLength(1); // re-attached, not reloaded
    expect(ctx.engine.load).toHaveBeenCalledTimes(1);
    expect(ctx.session.state.mode).toBe('full');
    expect(
      screen
        .getByRole('region', { name: /Video player/ })
        .querySelector('.cpl-stage')!
        .contains(ctx.video),
    ).toBe(true);
  });

  it('closing the mini player saves the position and stops', async () => {
    useCinemaApi();
    const ctx = await openAndPlay();
    media.setDuration(ctx.video, 6000);
    act(() => media.tick(ctx.video, 77));
    act(() => ctx.history.push('/support'));
    const mini = await screen.findByRole('region', { name: /Mini player/ });
    await ctx.user.click(within(mini).getByRole('button', { name: 'Close mini player' }));
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
    expect(ctx.progress.saved.at(-1)).toMatchObject({ titleId: 't1', positionS: 77 });
    expect(ctx.session.state.status).toBe('idle');
  });

  it('leaving a paused player ends the session instead of floating it', async () => {
    useCinemaApi();
    const ctx = await openAndPlay();
    await ctx.user.keyboard('k');
    act(() => ctx.history.push('/support'));
    await waitFor(() => expect(ctx.session.state.status).toBe('idle'));
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
  });

  it('the mini player passes axe', async () => {
    useCinemaApi();
    const ctx = await openAndPlay();
    act(() => ctx.history.push('/support'));
    await screen.findByRole('region', { name: /Mini player/ });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('playback errors', () => {
  it('offers to try again after a fatal media error', async () => {
    useCinemaApi();
    const ctx = await openAndPlay();
    act(() => ctx.engine.emit({ type: 'error', fatal: true, kind: 'media' }));
    expect(await screen.findByText('Playback stopped')).toBeInTheDocument();
    await ctx.user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(ctx.engine.load).toHaveBeenCalledTimes(2));
  });
});
