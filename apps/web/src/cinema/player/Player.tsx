// The full-screen player view (CIN-PLY-002..006, 008). The <video> itself lives in the session (session.ts);
// this component attaches it to its stage, draws the chrome and overlays, and hands it on to the mini player
// when the viewer navigates away while it is playing.
import { Button, EmptyState, Icon, Spinner } from '@sideroom/ui';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Controls } from './Controls';
import { PlayGlyph } from './icons';
import {
  STILL_WATCHING_AFTER,
  activeMarker,
  actionForKey,
  cueVars,
  nextCardStart,
  watchHref,
  type PlayerAction,
} from './model';
import { Feedback, NextCard, ResumePrompt, SkipButton, StillWatching } from './Overlays';
import { playerSession, usePlayer } from './session';
import './player.css';

const CHROME_HIDE_MS = 3000;

const isEditable = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));

export interface PlayerViewProps {
  /** Position of a saved session to offer ("Resume from 42:10"); null when there is nothing to offer. */
  resumeAt: number | null;
  onResumeHandled: () => void;
  /** In-app navigation (history push). */
  navigate: (href: string) => void;
  /** Leave the player (the back button, "Stop"). */
  onExit: () => void;
}

export function PlayerView({ resumeAt, onResumeHandled, navigate, onExit }: PlayerViewProps) {
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
  const feedback = usePlayer((x) => x.feedback);
  const autoplayNext = usePlayer((x) => x.autoplayNext);
  const autoplayedCount = usePlayer((x) => x.autoplayedCount);
  const subtitleStyle = usePlayer((x) => x.subtitleStyle);
  const error = usePlayer((x) => x.error);
  const engineKind = usePlayer((x) => x.engineKind) ?? 'none';

  const [chrome, setChrome] = useState(true);
  const [menuOpen, setMenuOpen] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [dismissedNext, setDismissedNext] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // The element belongs to the session: attach on mount, hand back (and decide mini vs close) on unmount.
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

  /* ---------------------------------------------------------- chrome */
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
  const chromeOn = chrome || !playing || menuOpen || needsGesture;

  /* ------------------------------------------------------ full screen */
  const fullscreenTarget = () =>
    (rootRef.current?.closest('[data-theme]') as HTMLElement | null) ?? rootRef.current;
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled;
  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void fullscreenTarget()?.requestFullscreen?.();
  }, []);
  useEffect(() => {
    const on = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', on);
    return () => document.removeEventListener('fullscreenchange', on);
  }, []);

  const video = s.element();
  const canPip = typeof document !== 'undefined' && !!document.pictureInPictureEnabled;
  const canRemote = 'remote' in video || 'webkitShowPlaybackTargetPicker' in video;
  const togglePip = () => {
    if (document.pictureInPictureElement) void document.exitPictureInPicture();
    else
      void video
        .requestPictureInPicture?.()
        .catch(() => s.say('Picture in picture is not available'));
  };
  const remote = () => {
    const w = video as HTMLVideoElement & {
      remote?: { prompt: () => Promise<void> };
      webkitShowPlaybackTargetPicker?: () => void;
    };
    if (w.webkitShowPlaybackTargetPicker) w.webkitShowPlaybackTargetPicker();
    else void w.remote?.prompt().catch(() => s.say('No devices found'));
  };

  /* --------------------------------------------------------- next card */
  const next = info?.next ?? null;
  const cardAt = info ? nextCardStart(info) : null;
  const showNext =
    !!info &&
    !!next &&
    cardAt !== null &&
    (currentTime >= cardAt || ended) &&
    dismissedNext !== info.media_id &&
    resumeAt === null;
  const chain = autoplayNext && autoplayedCount >= STILL_WATCHING_AFTER;

  const goNext = useCallback(
    (auto: boolean) => {
      if (!next) return;
      s.queueAutoNext(auto);
      navigate(watchHref(next.title_id, next.episode_id));
    },
    [next, s, navigate],
  );

  // Three episodes in a row without a touch: pause and ask, instead of counting down.
  useEffect(() => {
    if (showNext && chain && !asking) {
      s.pause();
      setAsking(true);
    }
  }, [showNext, chain, asking, s]);

  /* ---------------------------------------------------------- shortcuts */
  const run = useCallback(
    (a: PlayerAction) => {
      switch (a.type) {
        case 'toggle':
          s.toggle();
          break;
        case 'seekBy':
          s.seekBy(a.seconds);
          break;
        case 'seekFraction':
          s.seekFraction(a.fraction);
          break;
        case 'volumeBy':
          s.volumeBy(a.delta);
          break;
        case 'mute':
          s.toggleMute();
          break;
        case 'fullscreen':
          toggleFullscreen();
          break;
        case 'captions':
          s.toggleCaptions();
          break;
        case 'speed':
          s.stepRate(a.dir);
          break;
      }
    },
    [s, toggleFullscreen],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || isEditable(e.target)) return;
      const t = e.target instanceof HTMLElement ? e.target : null;
      if (t?.closest('[role="menu"],[role="dialog"]')) return;
      const a = actionForKey(e);
      if (!a) return;
      // Space on a focused button is that button's click; arrows on a focused slider move the slider.
      if (e.key === ' ' && t && (t.tagName === 'BUTTON' || t.tagName === 'A')) return;
      if (t instanceof HTMLInputElement && t.type === 'range' && e.key.startsWith('Arrow')) return;
      e.preventDefault();
      s.confirmStillWatching();
      poke();
      run(a);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [s, run, poke]);

  const marker =
    info && status === 'ready' && !showNext && resumeAt === null
      ? activeMarker(info.markers, currentTime)
      : null;

  const exitFullscreenThen = (fn: () => void) => {
    if (document.fullscreenElement) void document.exitFullscreen();
    fn();
  };

  return (
    // The wrapper only forwards pointer activity to the chrome timer and counts as "still watching".
    <div
      ref={rootRef}
      className="cpl-player"
      role="region"
      aria-label={info ? `Video player: ${info.title}` : 'Video player'}
      data-chrome={chromeOn ? 'on' : 'off'}
      data-engine={engineKind}
      style={cueVars(subtitleStyle) as React.CSSProperties}
      onPointerMove={poke}
      onPointerDown={() => s.confirmStillWatching()}
    >
      {/* Click-to-play is a pointer convenience; the keyboard route is the Play button and Space. */}
      <div
        ref={stageRef}
        className="cpl-stage"
        onClick={(e) => {
          if (e.target instanceof HTMLVideoElement) s.toggle();
        }}
        onDoubleClick={(e) => {
          if (e.target instanceof HTMLVideoElement) toggleFullscreen();
        }}
      />

      {!info && (
        <div className="cpl-center">
          <Spinner label="Opening title" />
        </div>
      )}
      {info && (
        <>
          <div className="cpl-top">
            <button
              type="button"
              className="cpl-btn"
              aria-label="Back"
              onClick={() => exitFullscreenThen(onExit)}
            >
              <Icon name="close" size={20} />
            </button>
            <div className="cpl-titleblock">
              <h1 className="cpl-title">{info.title}</h1>
              {info.episode_label && <p className="cpl-subtitle">{info.episode_label}</p>}
            </div>
          </div>

          {status === 'error' && (
            <div className="cpl-center">
              <EmptyState
                title="Playback stopped"
                description={error ?? 'Something went wrong while playing this.'}
                action={
                  <Button
                    onClick={() =>
                      void s.start(info, { startAt: s.state.currentTime, autoplay: true })
                    }
                  >
                    Try again
                  </Button>
                }
              />
            </div>
          )}

          {status !== 'error' && needsGesture && resumeAt === null && (
            <button
              type="button"
              className="cpl-bigplay"
              // A pointer shortcut for the Play button in the control bar; hidden from assistive tech so it is not announced twice.
              aria-hidden="true"
              tabIndex={-1}
              onClick={() => {
                s.confirmStillWatching();
                void s.play();
              }}
            >
              <PlayGlyph size={44} />
            </button>
          )}
          {status !== 'error' && (waiting || status === 'loading') && !needsGesture && (
            <div className="cpl-center" aria-hidden={false}>
              <Spinner label="Buffering" />
            </div>
          )}

          {resumeAt !== null && status !== 'error' && (
            <ResumePrompt
              at={resumeAt}
              onResume={() => {
                s.seek(resumeAt, false);
                onResumeHandled();
                void s.play();
              }}
              onStartOver={() => {
                s.seek(0, false);
                onResumeHandled();
                void s.play();
              }}
            />
          )}

          {marker && <SkipButton marker={marker} onSkip={() => s.seek(marker.end_s)} />}

          {showNext && next && !asking && (
            <NextCard
              key={info.media_id}
              next={next}
              autoplay={autoplayNext}
              onPlayNext={goNext}
              onCancel={() => setDismissedNext(info.media_id)}
            />
          )}

          {asking && (
            <StillWatching
              onContinue={() => {
                s.confirmStillWatching();
                setAsking(false);
                if (!s.state.ended) void s.play();
              }}
              onStop={() => {
                setAsking(false);
                exitFullscreenThen(onExit);
              }}
            />
          )}

          <Feedback feedback={feedback} />

          {status !== 'error' && (
            <Controls
              fullscreen={fullscreen}
              canFullscreen={canFullscreen}
              canPip={canPip}
              canRemote={canRemote}
              onFullscreen={toggleFullscreen}
              onPip={togglePip}
              onRemote={remote}
              onNext={() => goNext(false)}
              onMenuOpenChange={setMenuOpen}
            />
          )}

          {info.kind !== 'movie' && (
            <label className="cpl-autoplay">
              <input
                type="checkbox"
                checked={autoplayNext}
                onChange={(e) => s.setAutoplayNext(e.target.checked)}
              />
              <span>Autoplay next episode</span>
            </label>
          )}
        </>
      )}
    </div>
  );
}
