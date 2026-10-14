// The mini player (CIN-PLY-011): the session's own <video> in a small floating box, mounted once at the app root
// next to the voice bridge, so playback continues while the viewer moves around the app. `MiniPlayerView` takes
// plain props so watch parties can mount it for their theatre as well.
import { Icon, ThemeProvider } from '@sideroom/ui';
import type { RouterHistory } from '@tanstack/react-router';
import { useLayoutEffect, useRef } from 'react';
import { useAppliedTheme } from '../../settings/user/useAppliedTheme';
import { Expand, PauseGlyph, PlayGlyph } from './icons';
import { watchHref } from './model';
import { playerSession, usePlayer } from './session';
import './cinema-player.css';

export interface MiniPlayerViewProps {
  onExpand: () => void;
  onClose: () => void;
}

export function MiniPlayerView({ onExpand, onClose }: MiniPlayerViewProps) {
  const s = playerSession();
  const stage = useRef<HTMLDivElement>(null);
  const info = usePlayer((x) => x.info);
  const playing = usePlayer((x) => x.playing);
  const t = usePlayer((x) => x.currentTime);
  const d = usePlayer((x) => x.duration);

  useLayoutEffect(() => {
    const el = stage.current;
    if (!el) return;
    s.attach(el);
    return () => s.detach(el);
  }, [s]);

  if (!info) return null;
  return (
    <section className="cin-mini" aria-label={`Mini player: ${info.title}`}>
      <div ref={stage} className="cin-mini-stage" />
      <div className="cin-mini-bar">
        <button
          type="button"
          className="cin-mini-title"
          onClick={onExpand}
          aria-label={`Open ${info.title} full screen`}
        >
          {info.title}
          {info.episode_label ? ` · ${info.episode_label}` : ''}
        </button>
        <button
          type="button"
          className="cin-btn"
          aria-label={playing ? 'Pause' : 'Play'}
          onClick={() => s.toggle()}
        >
          {playing ? <PauseGlyph size={18} /> : <PlayGlyph size={18} />}
        </button>
        <button type="button" className="cin-btn" aria-label="Expand" onClick={onExpand}>
          <Expand />
        </button>
        <button type="button" className="cin-btn" aria-label="Close mini player" onClick={onClose}>
          <Icon name="close" size={16} />
        </button>
      </div>
      <div className="cin-mini-progress" aria-hidden="true">
        <div style={{ width: `${d > 0 ? Math.min(100, (t / d) * 100) : 0}%` }} />
      </div>
    </section>
  );
}

function Themed({ onExpand, onClose }: MiniPlayerViewProps) {
  const theme = useAppliedTheme();
  return (
    <ThemeProvider
      section="cinema"
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ background: 'transparent' }}
      {...theme.data}
      className="cin-mini-root"
    >
      <MiniPlayerView onExpand={onExpand} onClose={onClose} />
    </ThemeProvider>
  );
}

/**
 * Mounted once at the app root (outside the router, like the voice bridge). Shows itself when the session holds
 * an item that is not on screen in the full player.
 */
export function MiniPlayerHost({ history }: { history: RouterHistory }) {
  const status = usePlayer((x) => x.status);
  const mode = usePlayer((x) => x.mode);
  const info = usePlayer((x) => x.info);
  // The full player sets 'mini' when it is left while playing; until then it owns the picture.
  if (!info || status === 'idle' || mode !== 'mini') return null;
  const s = playerSession();
  return (
    <Themed
      onExpand={() => {
        history.push(watchHref(info.title_id, info.episode_id));
      }}
      onClose={() => s.close()}
    />
  );
}
