import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import { refreshSource } from '../../player/api';
import { MiniPlayerMount } from '../../player/MiniPlayerMount';
import { installMediaMocks, makeSession, media, resetSession } from '../../player/__tests__/fakes';
import { STALL_MS } from '../LiveView';
import { setLivePrefs } from '../livePrefs';
import { NOW, guide, useTvApi } from './fixtures';

beforeAll(() => {
  installMediaMocks();
  // The session store updates from media events, outside React's event system.
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = false;
});
afterEach(() => {
  resetSession();
  setLivePrefs({ crt: false, tuningStatic: true, tuningSound: false });
  vi.useRealTimers();
});

function renderLive(path = '/cinema/tv/watch/c101', initial: string[] = [path]) {
  const ctx = makeSession();
  const history = createMemoryHistory({ initialEntries: initial });
  const router = createAppRouter(history);
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
      <MiniPlayerMount history={router.history} />
    </QueryClientProvider>,
  );
  return { ...ctx, router, history, ...view, video: ctx.session.element() };
}

/** Pretend the stream reports a seekable range. */
function seekable(el: HTMLMediaElement, start: number, end: number) {
  Object.defineProperty(el, 'seekable', {
    configurable: true,
    get: () => ({ length: 1, start: () => start, end: () => end }),
  });
}

const player = () => screen.findByRole('region', { name: /Live TV: channel 101/ });

async function openAndPlay(path?: string) {
  const ctx = renderLive(path);
  const user = userEvent.setup();
  await player();
  await user.click(await screen.findByRole('button', { name: 'Play' }));
  await waitFor(() => expect(ctx.session.state.playing).toBe(true));
  return { ...ctx, user };
}

describe('live player', () => {
  it('tunes the channel, shows LIVE and now/next with progress, and has no scrubber; passes axe', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: NOW });
    const calls = useTvApi();
    const ctx = renderLive();
    const region = await player();
    expect(calls.tunes).toEqual(['c101']);
    await waitFor(() =>
      expect(ctx.engine.load).toHaveBeenCalledWith(
        '/api/v1/tv/channels/c101/live/master.m3u8',
        undefined,
      ),
    );
    expect(ctx.session.state.info?.live).toEqual({ channel_id: 'c101' });
    expect(
      await within(region).findByRole('heading', { name: 'Cowboy Bebop' }),
    ).toBeInTheDocument();
    const banner = within(region).getByRole('region', { name: 'Programme information' });
    expect(within(banner).getByText('Samurai Champloo', { exact: false })).toBeInTheDocument();
    // 15 of 30 minutes have aired at 20:15 server time.
    expect(within(banner).getByRole('progressbar', { name: 'Programme progress' })).toHaveAttribute(
      'aria-valuenow',
      '50',
    );
    // The OSD announces the channel.
    expect(await screen.findByText('101', { selector: '.ltv-osd-num' })).toBeInTheDocument();
    expect(await screen.findByTestId('live-badge')).toHaveTextContent('LIVE');
    // No seek bar without a DVR window.
    expect(screen.queryByRole('slider', { name: /Rewind/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Channel up' })).toBeInTheDocument();
    const found = await axeViolations(ctx.container);
    expect(found.map((v) => JSON.stringify(v).slice(0, 400))).toEqual([]);
  });

  it('never saves watch progress for a live channel', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    act(() => media.tick(ctx.video, 30));
    ctx.session.flush('heartbeat');
    ctx.session.flush('pause');
    expect(ctx.progress.saved).toEqual([]);
  });

  it('offers a rewind slider only when the playlist keeps a real window, never past the live edge', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    seekable(ctx.video, 100, 136); // a 36 s sliding window: no DVR
    act(() => media.tick(ctx.video, 135));
    await new Promise((r) => setTimeout(r, 1100));
    expect(screen.queryByRole('slider', { name: /Rewind/ })).not.toBeInTheDocument();
    seekable(ctx.video, 0, 600);
    act(() => media.tick(ctx.video, 599));
    const slider = await screen.findByRole(
      'slider',
      { name: 'Rewind within the live window' },
      { timeout: 3000 },
    );
    expect(slider).toHaveAttribute('max', '600');
    expect(slider).toHaveAttribute('aria-valuetext', 'At the live edge');
  });

  it('shows Back to live once behind the edge and jumps to the live position', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    seekable(ctx.video, 0, 600);
    act(() => media.tick(ctx.video, 300));
    const back = await screen.findByRole('button', { name: /Back to live/ }, { timeout: 3000 });
    expect(back).toHaveTextContent('5:00 behind');
    expect(screen.queryByTestId('live-badge')).not.toBeInTheDocument();
    await ctx.user.click(back);
    expect(media.time(ctx.video)).toBe(598);
    act(() => media.tick(ctx.video, 598));
    expect(await screen.findByTestId('live-badge')).toBeInTheDocument();
  });

  it('pauses and resumes from the keyboard', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    await ctx.user.keyboard('k');
    expect(media.isPaused(ctx.video)).toBe(true);
    await ctx.user.keyboard('k');
    expect(media.isPaused(ctx.video)).toBe(false);
  });

  it('changes channel with up and down (wrapping) and tunes the new one', async () => {
    const calls = useTvApi();
    const ctx = await openAndPlay();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Channel up' })).toBeEnabled());
    // wait for the channel list (guide) to load
    await waitFor(() => expect(calls.guide.length).toBeGreaterThan(0));
    await new Promise((r) => setTimeout(r, 50));
    await ctx.user.keyboard('{ArrowUp}');
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv/watch/c102'));
    await waitFor(() => expect(calls.tunes).toEqual(['c101', 'c102']));
    expect(await screen.findByRole('region', { name: /Live TV: channel 102/ })).toBeInTheDocument();
    // The picture changed channel without leaving the full player.
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
    await ctx.user.keyboard('{ArrowDown}{ArrowDown}');
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv/watch/c106'));
  });

  it('shows the tuning static between channels and respects the toggle', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    await new Promise((r) => setTimeout(r, 50));
    await ctx.user.click(screen.getByRole('button', { name: 'Channel up' }));
    expect(await screen.findByTestId('tuning-static')).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByTestId('tuning-static')).not.toBeInTheDocument(), {
      timeout: 3000,
    });
    setLivePrefs({ tuningStatic: false });
    await ctx.user.click(screen.getByRole('button', { name: 'Channel up' }));
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv/watch/c105'));
    expect(screen.queryByTestId('tuning-static')).not.toBeInTheDocument();
  });

  it('opens the channel list with G, moves with the arrows, tunes with Enter and closes with Escape', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: NOW });
    useTvApi();
    const ctx = await openAndPlay();
    await new Promise((r) => setTimeout(r, 50));
    await ctx.user.keyboard('g');
    const dialog = await screen.findByRole('dialog', { name: 'Channel guide' });
    const list = within(dialog).getByRole('listbox', { name: 'Channels' });
    await waitFor(() => expect(list).toHaveFocus());
    expect(within(list).getAllByRole('option')).toHaveLength(4);
    expect(within(list).getByRole('option', { name: /101/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(within(list).getByText("Dexter's Laboratory")).toBeInTheDocument();
    await ctx.user.keyboard('{ArrowDown}');
    expect(within(list).getByRole('option', { name: /102/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await ctx.user.keyboard('{Escape}');
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Channel guide' })).not.toBeInTheDocument(),
    );
    await ctx.user.keyboard('g');
    await screen.findByRole('dialog', { name: 'Channel guide' });
    await ctx.user.keyboard('{ArrowDown}{Enter}');
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv/watch/c102'));
    expect(screen.queryByRole('dialog', { name: 'Channel guide' })).not.toBeInTheDocument();
  });

  it('toggles the CRT filter from the keyboard and the settings panel, and remembers it', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    const region = await player();
    expect(region).not.toHaveClass('ltv-crt-on');
    await ctx.user.keyboard('c');
    expect(region).toHaveClass('ltv-crt-on');
    expect(JSON.parse(localStorage.getItem('sr.cinema.tv.v1')!).crt).toBe(true);
    await ctx.user.click(screen.getByRole('button', { name: 'Live settings' }));
    const box = screen.getByRole('checkbox', { name: 'CRT filter' });
    expect(box).toBeChecked();
    await ctx.user.click(box);
    expect(region).not.toHaveClass('ltv-crt-on');
    expect(screen.getByText(/no rewind window/)).toBeInTheDocument();
  });

  it('asks the age gate on 403 900011 and tunes after yes', async () => {
    let confirmed = false;
    const calls = useTvApi({
      tune: () =>
        confirmed
          ? undefined
          : HttpResponse.json(
              { code: 900011, message: 'Age confirmation required' },
              { status: 403 },
            ),
    });
    server.use(
      http.post('*/api/v1/sr/age-gate', () => {
        confirmed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const ctx = renderLive();
    const dialog = await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await userEvent.setup().click(within(dialog).getByRole('button', { name: 'Yes' }));
    await waitFor(() => expect(ctx.engine.load).toHaveBeenCalled());
    expect(calls.tunes).toEqual(['c101', 'c101']);
  });

  it('on no the player stays closed and the viewer goes back to the guide', async () => {
    useTvApi({
      tune: () =>
        HttpResponse.json({ code: 900011, message: 'Age confirmation required' }, { status: 403 }),
    });
    server.use(http.post('*/api/v1/sr/age-gate', () => new HttpResponse(null, { status: 204 })));
    const ctx = renderLive('/cinema/tv/watch/c101', ['/cinema/tv', '/cinema/tv/watch/c101']);
    const dialog = await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await userEvent.setup().click(within(dialog).getByRole('button', { name: 'No' }));
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv'));
    expect(ctx.engine.load).not.toHaveBeenCalled();
  });

  it('shows honest states when the channel is gone or Live TV is not available', async () => {
    useTvApi({ tune: () => HttpResponse.json({ message: 'nf' }, { status: 404, headers: {} }) });
    const first = renderLive();
    expect(await screen.findByText('This channel is not on the air')).toBeInTheDocument();
    first.unmount();
    resetSession();

    useTvApi({ tune: () => new HttpResponse(null, { status: 501 }) });
    renderLive();
    expect(await screen.findByText('Live TV is not available yet')).toBeInTheDocument();
  });

  it('says the channel is off the air when the server has nothing to tune', async () => {
    useTvApi({
      tune: () =>
        HttpResponse.json({ channel_id: 'c101', state: 'off_air', current: null, viewers: 0 }),
    });
    const ctx = renderLive();
    expect(await screen.findByText('This channel is off the air')).toBeInTheDocument();
    expect(ctx.engine.load).not.toHaveBeenCalled();
  });

  it('asks the session to refresh the signed URL by channel (tv:<id>) and applies it', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    await ctx.session.refreshNow(false);
    expect(ctx.refresh).toHaveBeenCalledWith('tv:c101');
    // The real refresh route is a tune.
    const calls = useTvApi();
    const fresh = await refreshSource('tv:c101');
    expect(calls.tunes).toEqual(['c101']);
    expect(fresh.master_url).toContain('master.m3u8');
    expect(fresh.expires_at_ms).toBeGreaterThan(Date.now());
  });

  it('reconnects after a stall by tuning again, then gives up with a retry button', async () => {
    const calls = useTvApi();
    const ctx = await openAndPlay();
    vi.useFakeTimers({ shouldAdvanceTime: true, toFake: ['setTimeout', 'clearTimeout'] });
    act(() => {
      ctx.video.dispatchEvent(new Event('waiting'));
    });
    expect(await screen.findByText('Signal lost. Reconnecting…')).toBeInTheDocument();
    for (let i = 1; i <= 3; i++) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(STALL_MS + 100);
      });
      await waitFor(() => expect(calls.tunes.length).toBe(1 + i));
      await waitFor(() => expect(ctx.engine.load).toHaveBeenCalledTimes(1 + i));
      await act(async () => {
        ctx.video.dispatchEvent(new Event('waiting'));
      });
    }
    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALL_MS + 100);
    });
    expect(await screen.findByText('The signal dropped')).toBeInTheDocument();
    vi.useRealTimers();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(calls.tunes.length).toBe(5));
  });

  it('shows an error with retry when the engine fails fatally', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    act(() => ctx.engine.emit({ type: 'error', fatal: true, kind: 'media' }));
    expect(await screen.findByText('The signal dropped')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });
});

describe('live mini player', () => {
  it('keeps the channel playing in the mini player and expands back to the live route', async () => {
    const calls = useTvApi();
    const ctx = await openAndPlay();
    act(() => ctx.history.push('/support'));
    const mini = await screen.findByRole('region', { name: /Mini player: Cowboy Bebop/ });
    expect(mini.querySelector('video')).toBe(ctx.video);
    expect(ctx.session.state.mode).toBe('mini');
    await ctx.user.click(within(mini).getByRole('button', { name: 'Expand' }));
    await waitFor(() => expect(ctx.router.state.location.pathname).toBe('/cinema/tv/watch/c101'));
    expect(await player()).toBeInTheDocument();
    // Re-attached, not tuned again.
    expect(calls.tunes).toEqual(['c101']);
    expect(ctx.engine.load).toHaveBeenCalledTimes(1);
  });

  it('closing a paused live player ends the session (no mini player)', async () => {
    useTvApi();
    const ctx = await openAndPlay();
    await ctx.user.keyboard('k');
    act(() => ctx.history.push('/support'));
    await waitFor(() => expect(ctx.session.state.status).toBe('idle'));
    expect(screen.queryByRole('region', { name: /Mini player/ })).not.toBeInTheDocument();
  });
});

it('fixtures are consistent', () => {
  expect(NOW).toBeGreaterThan(0);
  expect(guide().channels).toHaveLength(4);
});
