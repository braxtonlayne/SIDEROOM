// The theatre's control bar (WP-FE-003): the host and controllers drive playback through the server; everybody
// else sees the same bar with the playback controls disabled and a "Request control" button. Volume, captions
// and full screen are always the viewer's own.
import { Icon } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { formatClock } from '../cinema/player/model';
import { playerSession, usePlayer } from '../cinema/player/session';
import { partyController, useParty } from './runtime';
import { clock } from './sync';

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];
export const EMOJI = ['\u{1f37f}', '\u{1f602}', '\u{1f62e}', '❤️', '\u{1f44f}'];
const REACT_GAP_MS = 400;

export interface ControlBarProps {
  onFullscreen: () => void;
  canFullscreen: boolean;
  onShare: () => void;
}

export function ControlBar({ onFullscreen, canFullscreen, onShare }: ControlBarProps) {
  const tr = useT();
  const c = partyController();
  const s = playerSession();
  const party = useParty((x) => x.party);
  const me = useParty((x) => x.party?.you);
  const volume = usePlayer((x) => x.volume);
  const muted = usePlayer((x) => x.muted);
  const subtitleId = usePlayer((x) => x.subtitleId);
  const tracks = usePlayer((x) => x.info?.subtitle_tracks.length ?? 0);
  const t = usePlayer((x) => x.currentTime);
  const [scrub, setScrub] = useState<number | null>(null);
  const [cooling, setCooling] = useState(false);

  useEffect(() => {
    if (!cooling) return;
    const id = setTimeout(() => setCooling(false), REACT_GAP_MS);
    return () => clearTimeout(id);
  }, [cooling]);

  if (!party) return null;
  const pb = party.playback;
  const can = !!me?.can_control;
  const running = pb.playing || pb.auto_paused;
  const live = pb.live;
  const durMs = pb.duration_ms ?? 0;
  const posMs = scrub ?? t * 1000;
  const why = can ? undefined : tr('watchparty.controls.hostOnly');

  return (
    <div className="wp-controls" role="group" aria-label={tr('watchparty.controls.label')}>
      <button
        type="button"
        className="wp-btn wp-btn-main"
        aria-label={
          running ? tr('watchparty.controls.pauseAll') : tr('watchparty.controls.playAll')
        }
        aria-disabled={!can || live}
        title={why}
        onClick={() => can && !live && void c.toggle()}
      >
        <Icon name={running ? 'pause' : 'play'} size={24} />
      </button>
      <button
        type="button"
        className="wp-btn"
        aria-label={tr('watchparty.controls.back10')}
        aria-disabled={!can || live}
        title={why}
        onClick={() => can && !live && void c.seek(posMs - 10_000)}
      >
        −10
      </button>
      <button
        type="button"
        className="wp-btn"
        aria-label={tr('watchparty.controls.forward10')}
        aria-disabled={!can || live}
        title={why}
        onClick={() => can && !live && void c.seek(posMs + 10_000)}
      >
        +10
      </button>

      <span className="wp-time" aria-hidden="true">
        {live
          ? tr('watchparty.controls.live')
          : `${formatClock(posMs / 1000)} / ${formatClock(durMs / 1000)}`}
      </span>
      {!live && (
        <input
          className="wp-scrub"
          type="range"
          aria-label={tr('watchparty.controls.position')}
          aria-valuetext={tr('watchparty.controls.positionText', {
            pos: clock(posMs),
            dur: clock(durMs),
          })}
          min={0}
          max={Math.max(1, Math.round(durMs / 1000))}
          step={1}
          disabled={!can}
          title={why}
          value={Math.min(Math.round(posMs / 1000), Math.max(1, Math.round(durMs / 1000)))}
          onChange={(e) => setScrub(Number(e.target.value) * 1000)}
          onPointerUp={() => {
            if (scrub !== null) void c.seek(scrub);
            setScrub(null);
          }}
          onKeyUp={(e) => {
            if (
              scrub !== null &&
              (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End')
            ) {
              void c.seek(scrub);
              setScrub(null);
            }
          }}
          onBlur={() => setScrub(null)}
        />
      )}

      {!live && (
        <label className="wp-inline">
          <span className="wp-sr-only">{tr('watchparty.controls.speed')}</span>
          <select
            aria-label={tr('watchparty.controls.speed')}
            value={String(pb.rate)}
            disabled={!can}
            title={why}
            onChange={(e) => void c.setRate(Number(e.target.value))}
          >
            {SPEEDS.map((r) => (
              <option key={r} value={String(r)}>
                {r}×
              </option>
            ))}
          </select>
        </label>
      )}

      {party.queue.length > 0 && (
        <button
          type="button"
          className="wp-btn"
          aria-label={tr('watchparty.controls.nextInQueue')}
          aria-disabled={!can}
          title={why}
          onClick={() => can && void c.skip()}
        >
          <Icon name="list" size={20} />
        </button>
      )}

      {!can && (
        <button
          type="button"
          className="wp-btn wp-btn-text"
          onClick={() => void c.requestControl()}
        >
          {tr('watchparty.controls.requestControl')}
        </button>
      )}

      <span className="wp-spacer" />

      <div className="wp-emoji" role="group" aria-label={tr('watchparty.controls.sendReaction')}>
        {EMOJI.map((e) => (
          <button
            key={e}
            type="button"
            className="wp-btn wp-emoji-btn"
            aria-label={tr('watchparty.controls.react', { emoji: e })}
            disabled={cooling}
            onClick={() => {
              setCooling(true);
              void c.react(e);
            }}
          >
            {e}
          </button>
        ))}
      </div>

      <button
        type="button"
        className="wp-btn"
        aria-label={
          muted || volume === 0 ? tr('watchparty.controls.unmute') : tr('watchparty.controls.mute')
        }
        aria-pressed={muted}
        onClick={() => s.toggleMute()}
      >
        <Icon name="volume" size={20} />
      </button>
      <input
        className="wp-vol"
        type="range"
        aria-label={tr('watchparty.controls.volume')}
        min={0}
        max={100}
        value={Math.round((muted ? 0 : volume) * 100)}
        onChange={(e) => s.setVolume(Number(e.target.value) / 100, false)}
      />
      {tracks > 0 && (
        <button
          type="button"
          className="wp-btn wp-btn-text"
          aria-pressed={subtitleId !== null}
          onClick={() => s.toggleCaptions()}
        >
          {tr('watchparty.controls.captions')}
        </button>
      )}
      <button
        type="button"
        className="wp-btn"
        aria-label={tr('watchparty.controls.invite')}
        onClick={onShare}
      >
        <Icon name="link" size={20} />
      </button>
      {canFullscreen && (
        <button
          type="button"
          className="wp-btn"
          aria-label={tr('watchparty.controls.fullScreen')}
          onClick={onFullscreen}
        >
          <Icon name="grid" size={20} />
        </button>
      )}
    </div>
  );
}
