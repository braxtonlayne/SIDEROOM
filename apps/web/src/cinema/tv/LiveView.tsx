// The live player view (TV-PLY-001..004). The <video> lives in the shared player session (cinema/player/session.ts),
// so the mini player works exactly as for films; this component draws the live chrome: LIVE badge and "Back to
// live", a DVR scrubber only when the playlist keeps a real window, channel up/down, the channel list (G), the
// now/next banner, reconnect and stall handling, CRT filter and tuning static.
import { Button, EmptyState, Icon, Spinner } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { formatClock } from '../player/model';
import { PauseGlyph, PlayGlyph, VolumeGlyph, Fullscreen, ExitFullscreen } from '../player/icons';
import { Feedback } from '../player/Overlays';
import { playerSession, usePlayer } from '../player/session';
import { getChannelNow, getGuide, tvKeys } from './api';
import { ChannelList, ChannelOsd, InfoBanner, TuningStatic } from './LiveOverlays';
import { playStaticBurst, setLivePrefs, useLivePrefs } from './livePrefs';
import {
  HOUR,
  atLiveEdge,
  behindLive,
  hasDvr,
  nowAndNext,
  stepChannel,
  type LiveWindow,
} from './model';
import type { TuneGrant } from './types';
import '../player/player.css';
import './live.css';

const CHROME_HIDE_MS = 3000;
const OSD_MS = 3000;
const STATIC_MS = 600;
/** Waiting longer than this is a stall: tune again. */
export const STALL_MS = 10_000;
export const MAX_RECONNECTS = 3;
const STABLE_MS = 5000;

const isEditable = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));

/** The seekable range of the live stream (null until the browser reports one). */
function useLiveWindow(video: HTMLVideoElement): LiveWindow | null {
  const [w, setW] = useState<LiveWindow | null>(null);
  useEffect(() => {
    const read = () => {
      const r = video.seekable;
      if (!r || r.length === 0) return setW(null);
      const next = { start: r.start(0), end: r.end(r.length - 1) };
      setW((p) => (p && p.start === next.start && p.end === next.end ? p : next));
    };
    read();
    const id = setInterval(read, 1000);
    video.addEventListener('timeupdate', read);
    video.addEventListener('progress', read);
    return () => {
      clearInterval(id);
      video.removeEventListener('timeupdate', read);
      video.removeEventListener('progress', read);
    };
  }, [video]);
  return w;
}

export interface LiveViewProps {
  channelId: string;
  grant: TuneGrant | null;
  /** The channel is being tuned (grant request or session start). */
  tuning: boolean;
  /** Why tuning failed, when it did. */
  failure: { title: string; description: string } | null;
  onRetune: () => void;
  onSwitch: (channelId: string) => void;
  onExit: () => void;
}

export function LiveView({
  channelId,
  grant,
  tuning,
  failure,
  onRetune,
  onSwitch,
  onExit,
}: LiveViewProps) {
  const s = playerSession();
  const rootRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const info = usePlayer((x) => x.info);
  const status = usePlayer((x) => x.status);
  const playing = usePlayer((x) => x.playing);
  const waiting = usePlayer((x) => x.waiting);
  const ended = usePlayer((x) => x.ended);
  const needsGesture = usePlayer((x) => x.needsGesture);
  const currentTime = usePlayer((x) => x.currentTime);
  const muted = usePlayer((x) => x.muted);
  const volume = usePlayer((x) => x.volume);
  const feedback = usePlayer((x) => x.feedback);
  const prefs = useLivePrefs();
  const video = s.element();
  const win = useLiveWindow(video);

  const [chrome, setChrome] = useState(true);
  const [fullscreen, setFullscreen] = useState(false);
  const [guideOpen, setGuideOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [reconnects, setReconnects] = useState(0);
  const retuneRef = useRef(onRetune);
  retuneRef.current = onRetune;
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // The element belongs to the session: attach on mount, hand back (mini player or close) on unmount.
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    s.attach(stage);
    s.setMode('full');
    return () => {
      s.detach(stage);
      const st = s.state;
      if (st.info && st.playing) s.setMode('mini');
      else s.close();
    };
  }, [s]);

  /* ---- clock (server time, so every viewer shows the same programme) ---- */
  const offset = useRef(0);
  if (grant) offset.current = grant.server_time_ms - Date.now();
  const [tick, setTick] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setTick(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const nowMs = tick + offset.current;

  /* ---- what is on ---- */
  const nowQ = useQuery({
    queryKey: tvKeys.now(channelId),
    queryFn: () => getChannelNow(channelId),
    refetchInterval: 30_000,
    retry: false,
    staleTime: 15_000,
  });
  const guideQ = useQuery({
    queryKey: ['cinema', 'tv', 'live-guide'],
    queryFn: () => getGuide(Date.now() - HOUR, Date.now() + 3 * HOUR),
    staleTime: 60_000,
    retry: false,
  });
  const channels = useMemo(() => guideQ.data?.channels ?? [], [guideQ.data]);
  const channel = grant?.channel ?? channels.find((c) => c.id === channelId) ?? null;
  const fromGuide = nowAndNext(guideQ.data?.blocks[channelId], nowMs);
  const now = nowQ.data?.now ?? grant?.now ?? fromGuide.now;
  const next = nowQ.data?.next ?? grant?.next ?? fromGuide.next;

  // Keep the session (and so the mini player) titled with what is on now.
  const nowTitle = now?.title ?? null;
  useEffect(() => {
    const cur = s.state.info;
    if (!cur?.live || cur.live.channel_id !== channelId || !nowTitle || cur.title === nowTitle)
      return;
    s.store.setState({ info: { ...cur, title: nowTitle } });
  }, [s, channelId, nowTitle]);

  /* ---- chrome ---- */
  const poke = useCallback(() => {
    setChrome(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setChrome(false), CHROME_HIDE_MS);
  }, []);
  useEffect(() => {
    poke();
    return () => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
    };
  }, [poke]);
  const chromeOn = chrome || !playing || guideOpen || settingsOpen || needsGesture;

  /* ---- OSD and tuning static on channel change ---- */
  const [osd, setOsd] = useState(true);
  const [flash, setFlash] = useState(false);
  const first = useRef(true);
  useEffect(() => {
    setOsd(true);
    const t = setTimeout(() => setOsd(false), OSD_MS);
    let f: ReturnType<typeof setTimeout> | undefined;
    if (!first.current) {
      setFlash(true);
      f = setTimeout(() => setFlash(false), STATIC_MS);
      if (useLivePrefs.getState().tuningSound) playStaticBurst();
    }
    first.current = false;
    setReconnects(0);
    return () => {
      clearTimeout(t);
      if (f) clearTimeout(f);
    };
  }, [channelId]);
  const showStatic = prefs.tuningStatic && (flash || (tuning && !first.current));

  /* ---- full screen ---- */
  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else
      void (
        (rootRef.current?.closest('[data-theme]') as HTMLElement | null) ?? rootRef.current
      )?.requestFullscreen?.();
  }, []);
  useEffect(() => {
    const on = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', on);
    return () => document.removeEventListener('fullscreenchange', on);
  }, []);
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled;

  /* ---- live edge ---- */
  const dvr = hasDvr(win);
  const behind = behindLive(win, currentTime);
  const live = atLiveEdge(win, currentTime);
  const goLive = useCallback(() => {
    const w = win;
    if (w) s.seek(Math.max(w.start, w.end - 2), false);
    void s.play();
    s.say('Back to live');
  }, [s, win]);

  /* ---- stalls and reconnects ---- */
  const gaveUp = reconnects >= MAX_RECONNECTS;
  const stalled = status === 'ready' && ((waiting && !needsGesture) || ended);
  useEffect(() => {
    if (!stalled || gaveUp) return;
    const t = setTimeout(
      () => {
        setReconnects((n) => n + 1);
        retuneRef.current();
      },
      ended ? 1000 : STALL_MS,
    );
    return () => clearTimeout(t);
  }, [stalled, gaveUp, ended, reconnects]);
  // The count starts over once the picture has held for a few seconds, not at the first frame.
  useEffect(() => {
    if (!playing || waiting) return;
    const t = setTimeout(() => setReconnects(0), STABLE_MS);
    return () => clearTimeout(t);
  }, [playing, waiting]);
  const reconnecting = stalled && !gaveUp;

  /* ---- channels ---- */
  const step = useCallback(
    (dir: 1 | -1) => {
      const c = stepChannel(channels, channelId, dir);
      if (c && c.id !== channelId) onSwitch(c.id);
    },
    [channels, channelId, onSwitch],
  );
  const tuneFromGuide = (id: string) => {
    setGuideOpen(false);
    if (id !== channelId) onSwitch(id);
  };

  /* ---- keys ---- */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || isEditable(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target instanceof HTMLElement ? e.target : null;
      if (t?.closest('[role="menu"],[role="listbox"]')) return;
      const onButton = !!t && (t.tagName === 'BUTTON' || t.tagName === 'A');
      let handled = true;
      switch (e.key) {
        case ' ':
          if (onButton) return;
          s.toggle();
          break;
        case 'k':
        case 'K':
          s.toggle();
          break;
        case 'ArrowUp':
        case 'PageUp':
          if (t instanceof HTMLInputElement && t.type === 'range') return;
          step(1);
          break;
        case 'ArrowDown':
        case 'PageDown':
          if (t instanceof HTMLInputElement && t.type === 'range') return;
          step(-1);
          break;
        case 'g':
        case 'G':
          setGuideOpen((o) => !o);
          break;
        case 'm':
        case 'M':
          s.toggleMute();
          break;
        case 'f':
        case 'F':
          toggleFullscreen();
          break;
        case 'l':
        case 'L':
          goLive();
          break;
        case 'c':
        case 'C':
          setLivePrefs({ crt: !useLivePrefs.getState().crt });
          break;
        case '+':
        case '=':
          s.volumeBy(0.1);
          break;
        case '-':
          s.volumeBy(-0.1);
          break;
        case 'Escape':
          if (settingsOpen) setSettingsOpen(false);
          else if (guideOpen) setGuideOpen(false);
          else handled = false;
          break;
        default:
          handled = false;
      }
      if (handled) {
        e.preventDefault();
        poke();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [s, step, toggleFullscreen, goLive, poke, settingsOpen, guideOpen]);

  const leave = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    onExit();
  };
  const frameReady = !!info && status !== 'idle';
  const title = now?.title ?? info?.title ?? 'Live TV';
  const rootLabel = `Live TV: ${channel ? `channel ${channel.number} ${channel.call_sign}` : 'channel'}`;
  const volPct = Math.round((muted ? 0 : volume) * 100);
  const failed = failure ?? (status === 'error' || gaveUp ? STOPPED : null);
  const winSpan = win ? win.end - win.start : 0;
  const crtClass = useMemo(() => (prefs.crt ? 'ltv-crt-on' : ''), [prefs.crt]);

  return (
    <div
      ref={rootRef}
      className={`cpl-player ltv-player ${crtClass}`.trim()}
      role="region"
      aria-label={rootLabel}
      data-chrome={chromeOn ? 'on' : 'off'}
      data-live={live || undefined}
      onPointerMove={poke}
    >
      <div
        ref={stageRef}
        className="cpl-stage ltv-stage"
        onClick={(e) => {
          if (e.target instanceof HTMLVideoElement) s.toggle();
        }}
        onDoubleClick={(e) => {
          if (e.target instanceof HTMLVideoElement) toggleFullscreen();
        }}
      />
      {prefs.crt && <div className="ltv-crt" aria-hidden="true" />}
      <TuningStatic visible={showStatic} />

      <div className="cpl-top">
        <button type="button" className="cpl-btn" aria-label="Back to the guide" onClick={leave}>
          <Icon name="close" size={20} />
        </button>
        <div className="cpl-titleblock">
          <h1 className="cpl-title">{title}</h1>
          <p className="cpl-subtitle">
            {channel ? `Channel ${channel.number} · ${channel.call_sign}` : 'Live TV'}
          </p>
        </div>
      </div>

      <ChannelOsd channel={channel} title={nowTitle} visible={osd && !failed} />

      {failed && (
        <div className="cpl-center">
          <EmptyState
            title={failed.title}
            description={failed.description}
            action={
              <Button
                onClick={() => {
                  setReconnects(0);
                  onRetune();
                }}
              >
                Try again
              </Button>
            }
          />
        </div>
      )}

      {!failed && needsGesture && (
        <button
          type="button"
          className="cpl-bigplay"
          aria-hidden="true"
          tabIndex={-1}
          onClick={() => void s.play()}
        >
          <PlayGlyph size={44} />
        </button>
      )}
      {!failed && (tuning || (waiting && !needsGesture) || status === 'loading') && (
        <div className="cpl-center">
          {reconnecting ? (
            <p role="status" className="ltv-reconnect">
              Signal lost. Reconnecting…
            </p>
          ) : (
            <Spinner label={tuning ? 'Tuning' : 'Buffering'} />
          )}
        </div>
      )}
      <Feedback feedback={feedback} />

      {guideOpen && (
        <ChannelList
          channels={channels}
          blocks={guideQ.data?.blocks ?? {}}
          currentId={channelId}
          nowMs={nowMs}
          onTune={tuneFromGuide}
          onClose={() => setGuideOpen(false)}
        />
      )}

      {frameReady && !failed && (
        <div className="cpl-controls">
          <InfoBanner channel={channel} now={now} next={next} nowMs={nowMs} />
          <div className="cpl-row">
            <button
              type="button"
              className="cpl-btn"
              aria-label={playing ? 'Pause' : 'Play'}
              onClick={() => s.toggle()}
            >
              {playing ? <PauseGlyph /> : <PlayGlyph />}
            </button>
            <button
              type="button"
              className="cpl-btn"
              aria-label="Channel down"
              aria-keyshortcuts="ArrowDown"
              onClick={() => step(-1)}
            >
              <Icon name="chevron-down" size={20} />
            </button>
            <button
              type="button"
              className="cpl-btn"
              aria-label="Channel up"
              aria-keyshortcuts="ArrowUp"
              onClick={() => step(1)}
            >
              <Icon name="chevron-up" size={20} />
            </button>
            <button
              type="button"
              className="cpl-btn"
              aria-label={muted ? 'Unmute' : 'Mute'}
              onClick={() => s.toggleMute()}
            >
              <VolumeGlyph muted={muted || volume === 0} level={volume} />
            </button>
            <input
              type="range"
              className="ltv-volume"
              aria-label="Volume"
              min={0}
              max={100}
              value={volPct}
              onChange={(e) => s.setVolume(Number(e.target.value) / 100, false)}
            />
            {live ? (
              <span className="ltv-live" data-testid="live-badge">
                <span aria-hidden="true">●</span> LIVE
              </span>
            ) : (
              <button type="button" className="ltv-golive" onClick={goLive}>
                Back to live
                <span className="cpl-sr-only">, {formatClock(behind)} behind</span>
              </button>
            )}
            {dvr && win && (
              <input
                type="range"
                className="ltv-dvr"
                aria-label="Rewind within the live window"
                aria-valuetext={
                  behind <= LIVE_EDGE_LABEL_S
                    ? 'At the live edge'
                    : `${formatClock(behind)} behind live`
                }
                min={Math.floor(win.start)}
                max={Math.floor(win.end)}
                step={1}
                value={Math.min(Math.max(currentTime, win.start), win.end)}
                onChange={(e) => s.seek(Number(e.target.value), false)}
              />
            )}
            {dvr && (
              <span className="cpl-time" aria-hidden="true">
                {behind > LIVE_EDGE_LABEL_S ? `−${formatClock(behind)}` : ''}
              </span>
            )}
            <span className="cpl-spacer" />
            <button
              type="button"
              className="cpl-btn ltv-textbtn"
              aria-pressed={guideOpen}
              aria-keyshortcuts="g"
              onClick={() => setGuideOpen((o) => !o)}
            >
              Guide
            </button>
            <button
              type="button"
              className="cpl-btn"
              aria-label="Live settings"
              aria-expanded={settingsOpen}
              onClick={() => setSettingsOpen((o) => !o)}
            >
              <Icon name="settings" size={20} />
            </button>
            {canFullscreen && (
              <button
                type="button"
                className="cpl-btn"
                aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}
                onClick={toggleFullscreen}
              >
                {fullscreen ? <ExitFullscreen /> : <Fullscreen />}
              </button>
            )}
          </div>
          {settingsOpen && (
            <div className="ltv-settings" role="group" aria-label="Live settings">
              <label>
                <input
                  type="checkbox"
                  checked={prefs.crt}
                  onChange={(e) => setLivePrefs({ crt: e.target.checked })}
                />
                CRT filter
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={prefs.tuningStatic}
                  onChange={(e) => setLivePrefs({ tuningStatic: e.target.checked })}
                />
                Static between channels
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={prefs.tuningSound}
                  disabled={!prefs.tuningStatic}
                  onChange={(e) => setLivePrefs({ tuningSound: e.target.checked })}
                />
                Static sound
              </label>
              <p className="ltv-settings-note">
                {dvr
                  ? `This channel keeps ${formatClock(winSpan)} to rewind.`
                  : 'This channel has no rewind window; pause and Back to live still work.'}
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

const LIVE_EDGE_LABEL_S = 12;
const STOPPED = {
  title: 'The signal dropped',
  description: 'We could not keep this channel playing. Check your connection and try again.',
};
