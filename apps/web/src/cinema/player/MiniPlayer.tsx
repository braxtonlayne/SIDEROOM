// The mini player (CIN-PLY-011): the session's own <video> in a small floating box, mounted once at the app root
// next to the voice bridge, so playback continues while the viewer moves around the app. `MiniPlayerView` takes
// plain props so watch parties can mount it for their theatre as well.
import { Icon, ThemeProvider } from '@sideroom/ui';
import type { RouterHistory } from '@tanstack/react-router';
import { useLayoutEffect, useRef } from 'react';
import { useT } from '../../i18n';
import { useAppliedTheme } from '../../settings/user/useAppliedTheme';
import { useMiniPlayerActive } from './bridge';
import { Expand, PauseGlyph, PlayGlyph } from './icons';
import { watchHref } from './model';
import { playerSession, usePlayer } from './session';
import './player.css';

export interface MiniPlayerViewProps {
  onExpand: () => void;
  onClose: () => void;
}

export function MiniPlayerView({ onExpand, onClose }: MiniPlayerViewProps) {
  const tr = useT();
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
    <section className="cpl-mini" aria-label={tr('cinema.mini.label', { title: info.title })}>
      <div ref={stage} className="cpl-mini-stage" />
      <div className="cpl-mini-bar">
        <button
          type="button"
          className="cpl-mini-title"
          onClick={onExpand}
          aria-label={tr('cinema.mini.open', { title: info.title })}
        >
          {info.title}
          {info.episode_label ? ` · ${info.episode_label}` : ''}
        </button>
        <button
          type="button"
          className="cpl-btn"
          aria-label={playing ? tr('cinema.mini.pause') : tr('cinema.mini.play')}
          onClick={() => s.toggle()}
        >
          {playing ? <PauseGlyph size={18} /> : <PlayGlyph size={18} />}
        </button>
        <button
          type="button"
          className="cpl-btn"
          aria-label={tr('cinema.mini.expand')}
          onClick={onExpand}
        >
          <Expand />
        </button>
        <button
          type="button"
          className="cpl-btn"
          aria-label={tr('cinema.mini.close')}
          onClick={onClose}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
      <div className="cpl-mini-progress" aria-hidden="true">
        <div style={{ width: `${d > 0 ? Math.min(100, (t / d) * 100) : 0}%` }} />
      </div>
    </section>
  );
}

function Themed({ onExpand, onClose }: MiniPlayerViewProps) {
  const theme = useAppliedTheme(undefined, 'cinema');
  return (
    <ThemeProvider
      section="cinema"
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ background: 'transparent' }}
      {...theme.data}
      className="cpl-mini-root"
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
  const expandHref = useMiniPlayerActive((x) => x.expandHref);
  // The full player sets 'mini' when it is left while playing; until then it owns the picture.
  if (!info || status === 'idle' || mode !== 'mini') return null;
  const s = playerSession();
  return (
    <Themed
      onExpand={() => {
        history.push(
          expandHref ??
            (info.live
              ? `/cinema/tv/watch/${encodeURIComponent(info.live.channel_id)}`
              : watchHref(info.title_id, info.episode_id)),
        );
      }}
      onClose={() => s.close()}
    />
  );
}
