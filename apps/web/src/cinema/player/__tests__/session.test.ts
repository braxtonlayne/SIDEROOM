import { vi } from 'vitest';
import { setAssRendererLoader } from '../ass';
import { LEVELS, installMediaMocks, makeInfo, makeSession, media, resetSession } from './fakes';

beforeAll(installMediaMocks);
afterEach(() => {
  vi.useRealTimers();
  resetSession();
});

async function started(o: Parameters<typeof makeInfo>[0] = {}, play = true) {
  const ctx = makeSession();
  const info = makeInfo(o);
  await ctx.session.start(info, { autoplay: play });
  return { ...ctx, info, video: ctx.session.element() };
}

function resetKeepingPrefs() {
  const prefs = localStorage.getItem('sr.cinema.player.v1');
  resetSession();
  if (prefs) localStorage.setItem('sr.cinema.player.v1', prefs);
}

describe('start', () => {
  it('loads the master playlist and waits for a gesture when not autoplaying', async () => {
    const { session, engine, info } = await started({}, false);
    expect(engine.load).toHaveBeenCalledWith(info.master_url, undefined);
    expect(engine.setMaxHeight).toHaveBeenCalledWith(null);
    expect(session.state.status).toBe('ready');
    expect(session.state.needsGesture).toBe(true);
    expect(session.state.playing).toBe(false);
  });

  it('starts at the requested position and plays when asked', async () => {
    const ctx = makeSession();
    await ctx.session.start(makeInfo(), { startAt: 120, autoplay: true });
    expect(ctx.engine.load).toHaveBeenCalledWith(expect.any(String), 120);
    expect(ctx.session.state.playing).toBe(true);
  });

  it('keeps the data saver preference across sessions', async () => {
    const a = await started();
    a.session.setDataSaver(true);
    expect(a.engine.setMaxHeight).toHaveBeenLastCalledWith(480);
    resetKeepingPrefs();
    const b = makeSession();
    await b.session.start(makeInfo(), {});
    expect(b.engine.setMaxHeight).toHaveBeenCalledWith(480);
  });
});

describe('progress saving (CIN-PLY-007)', () => {
  it('saves on a 10 s heartbeat while playing', async () => {
    vi.useFakeTimers();
    const { progress, video } = await started();
    media.setDuration(video, 6000);
    media.tick(video, 40);
    vi.advanceTimersByTime(10_000);
    expect(progress.saved).toHaveLength(1);
    expect(progress.saved[0]).toMatchObject({
      mediaId: 'm1',
      titleId: 't1',
      positionS: 40,
      durationS: 6000,
      completed: false,
    });
    media.tick(video, 55);
    vi.advanceTimersByTime(10_000);
    expect(progress.saved).toHaveLength(2);
    expect(progress.saved[1]!.positionS).toBe(55);
  });

  it('saves on pause and when the tab is hidden, without repeating an unchanged position', async () => {
    const { session, progress, video } = await started();
    media.setDuration(video, 6000);
    media.tick(video, 100);
    session.pause();
    expect(progress.saved).toHaveLength(1);
    session.flush('heartbeat');
    expect(progress.saved).toHaveLength(1);
    media.tick(video, 140);
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(progress.saved).toHaveLength(2);
    expect(progress.api.save).toHaveBeenLastCalledWith(expect.anything(), { keepalive: true });
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
  });

  it('marks the item watched at 90 percent and at the end', async () => {
    const { session, progress, video } = await started();
    media.setDuration(video, 1000);
    media.tick(video, 905);
    session.flush('heartbeat');
    expect(progress.saved.at(-1)).toMatchObject({ completed: true, positionS: 1000 });
    media.end(video);
    expect(progress.saved.at(-1)).toMatchObject({ completed: true });
  });

  it('never saves an item the viewer did not play, and saves the replaced item when a new one starts', async () => {
    const ctx = await started({}, false);
    ctx.session.flush('close');
    expect(ctx.progress.saved).toHaveLength(0);
    await ctx.session.play();
    media.setDuration(ctx.video, 6000);
    media.tick(ctx.video, 300);
    await ctx.session.start(makeInfo({ media_id: 'm2', title_id: 't2' }), {});
    expect(ctx.progress.saved.at(-1)).toMatchObject({ mediaId: 'm1', positionS: 300 });
  });
});

describe('signed URL refresh (CIN-PLY-010)', () => {
  it('re-signs ahead of expiry without touching playback', async () => {
    vi.useFakeTimers();
    const now = Date.now();
    const ctx = makeSession();
    const info = makeInfo({ expires_at_ms: now + 6 * 3600_000 });
    await ctx.session.start(info, { autoplay: true });
    media.setDuration(ctx.session.element(), 6000);
    await vi.advanceTimersByTimeAsync(6 * 3600_000 - 5 * 60_000 - 1000);
    expect(ctx.refresh).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2000);
    expect(ctx.refresh).toHaveBeenCalledWith('m1');
    expect(ctx.engine.updateSigning).toHaveBeenCalledWith(expect.stringContaining('sig=b'));
    expect(ctx.engine.swapSource).not.toHaveBeenCalled();
    expect(media.isPaused(ctx.session.element())).toBe(false);
    expect(ctx.session.state.info!.master_url).toContain('sig=b');
  });

  it('swaps the source at the current position when the engine reports a refused request', async () => {
    const ctx = await started();
    media.tick(ctx.video, 321);
    ctx.engine.emit({ type: 'error', fatal: false, kind: 'auth', status: 403 });
    await vi.waitFor(() => expect(ctx.engine.swapSource).toHaveBeenCalled());
    expect(ctx.engine.swapSource).toHaveBeenCalledWith(expect.stringContaining('sig=b'), 321);
  });

  it('swaps instead of re-signing on native HLS, which cannot rewrite requests', async () => {
    vi.useFakeTimers();
    const ctx = makeSession();
    ctx.engine.kind = 'native';
    await ctx.session.start(makeInfo({ expires_at_ms: Date.now() + 3600_000 }), { autoplay: true });
    await vi.advanceTimersByTimeAsync(3600_000 - 5 * 60_000 + 10);
    expect(ctx.engine.swapSource).toHaveBeenCalled();
    expect(ctx.engine.updateSigning).not.toHaveBeenCalled();
  });

  it('retries a failed refresh while the old URL still works', async () => {
    vi.useFakeTimers();
    let n = 0;
    const ctx = makeSession({
      refresh: () => {
        n += 1;
        return n === 1
          ? Promise.reject(new Error('offline'))
          : Promise.resolve({
              master_url: 'https://cdn.test/m?sig=c',
              expires_at_ms: Date.now() + 3600_000,
            });
      },
    });
    await ctx.session.start(makeInfo({ expires_at_ms: Date.now() + 3600_000 }), { autoplay: true });
    await vi.advanceTimersByTimeAsync(3600_000 - 5 * 60_000 + 10);
    expect(ctx.refresh).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(30_001);
    expect(ctx.refresh).toHaveBeenCalledTimes(2);
    expect(ctx.engine.updateSigning).toHaveBeenCalledWith('https://cdn.test/m?sig=c');
  });
});

describe('controls', () => {
  it('seeks, clamps, changes speed and announces it', async () => {
    const { session, video } = await started();
    media.setDuration(video, 100);
    session.seekBy(10);
    expect(media.time(video)).toBe(10);
    expect(session.state.feedback?.text).toBe('+10 s');
    session.seek(500);
    expect(media.time(video)).toBe(100);
    session.seekFraction(0.5);
    expect(media.time(video)).toBe(50);
    session.stepRate(1);
    expect(media.rate(video)).toBe(1.25);
    expect(session.state.feedback?.text).toBe('1.25×');
  });

  it('adjusts volume, mutes and remembers both', async () => {
    const { session, video } = await started();
    session.setVolume(0.4);
    expect(session.state.volume).toBeCloseTo(0.4);
    session.toggleMute();
    expect(session.state.muted).toBe(true);
    expect(JSON.parse(localStorage.getItem('sr.cinema.player.v1')!)).toMatchObject({
      muted: true,
    });
    session.volumeBy(0.05);
    expect(video.muted).toBe(false); // raising the volume unmutes
  });

  it('forwards quality and audio choices to the engine and mirrors engine events', async () => {
    const { session, engine } = await started();
    engine.emit({ type: 'levels', levels: LEVELS });
    engine.emit({ type: 'level', index: 1 });
    engine.emit({
      type: 'audio',
      tracks: [
        { id: '0', label: 'English', language: 'en' },
        { id: '1', label: 'Japanese', language: 'ja' },
      ],
      active: '0',
    });
    expect(session.state.levels).toHaveLength(3);
    expect(session.state.playingLevel).toBe(1);
    session.setQuality(2);
    expect(engine.setQuality).toHaveBeenCalledWith(2);
    expect(session.state.feedback?.text).toBe('1080p');
    session.setQuality('auto');
    session.setAudioTrack('1');
    expect(engine.setAudioTrack).toHaveBeenCalledWith('1');
    expect(session.state.audioId).toBe('1');
  });

  it('shows the big play button when the browser refuses to start', async () => {
    const ctx = makeSession();
    await ctx.session.start(makeInfo(), {});
    const video = ctx.session.element();
    vi.spyOn(video, 'play').mockRejectedValueOnce(
      Object.assign(new Error('blocked'), { name: 'NotAllowedError' }),
    );
    await ctx.session.play();
    expect(ctx.session.state.needsGesture).toBe(true);
    await ctx.session.play();
    expect(ctx.session.state.needsGesture).toBe(false);
  });
});

describe('subtitles (CIN-PLY-004)', () => {
  it('adds a native WebVTT track, switches and removes it', async () => {
    const { session, video } = await started();
    await session.setSubtitle('s-en');
    let tracks = video.querySelectorAll('track');
    expect(tracks).toHaveLength(1);
    expect(tracks[0]).toHaveAttribute('src', 'https://cdn.test/m1/en.vtt');
    expect(tracks[0]).toHaveAttribute('srclang', 'en');
    await session.setSubtitle('s-ja');
    tracks = video.querySelectorAll('track');
    expect(tracks).toHaveLength(1);
    expect(tracks[0]).toHaveAttribute('src', 'https://cdn.test/m1/ja.vtt');
    await session.setSubtitle(null);
    expect(video.querySelectorAll('track')).toHaveLength(0);
  });

  it('toggles captions with C, remembering the last language', async () => {
    const { session, video } = await started();
    session.toggleCaptions();
    await vi.waitFor(() => expect(video.querySelectorAll('track')).toHaveLength(1));
    await session.setSubtitle('s-ja');
    session.toggleCaptions();
    await vi.waitFor(() => expect(video.querySelectorAll('track')).toHaveLength(0));
    session.toggleCaptions();
    await vi.waitFor(() => expect(video.querySelector('track')).toHaveAttribute('srclang', 'ja'));
  });

  it('falls back to WebVTT when an ASS track has no renderer, and uses the renderer when registered', async () => {
    const info = makeInfo({
      subtitle_tracks: [
        { id: 's1', language: 'en', label: 'English', vtt_url: '/en.vtt', ass_url: '/en.ass' },
      ],
    });
    const a = makeSession();
    await a.session.start(info, {});
    await a.session.setSubtitle('s1');
    expect(a.session.state.assActive).toBe(false);
    expect(a.session.element().querySelectorAll('track')).toHaveLength(1);

    const dispose = vi.fn();
    setAssRendererLoader(() => Promise.resolve({ dispose }));
    try {
      await a.session.setSubtitle('s1');
      expect(a.session.state.assActive).toBe(true);
      expect(a.session.element().querySelectorAll('track')).toHaveLength(0);
      await a.session.setSubtitle(null);
      expect(dispose).toHaveBeenCalled();
    } finally {
      setAssRendererLoader(null);
    }
  });

  it('keeps the styling choice', async () => {
    const { session } = await started();
    session.setSubtitleStyle({ size: 'large', color: 'yellow' });
    expect(session.state.subtitleStyle).toMatchObject({ size: 'large', color: 'yellow' });
    expect(JSON.parse(localStorage.getItem('sr.cinema.player.v1')!).subtitleStyle.size).toBe(
      'large',
    );
  });
});

describe('auto-played chain', () => {
  it('counts episodes that began by themselves and resets on a manual start', async () => {
    const { session } = await started();
    session.queueAutoNext(true);
    await session.start(makeInfo({ media_id: 'm2' }), { autoplay: true });
    session.queueAutoNext(true);
    await session.start(makeInfo({ media_id: 'm3' }), { autoplay: true });
    expect(session.state.autoplayedCount).toBe(2);
    session.confirmStillWatching();
    expect(session.state.autoplayedCount).toBe(0);
    session.queueAutoNext(true);
    await session.start(makeInfo({ media_id: 'm4' }), { autoplay: true });
    await session.start(makeInfo({ media_id: 'm5' }), { autoplay: true });
    expect(session.state.autoplayedCount).toBe(0);
  });
});
