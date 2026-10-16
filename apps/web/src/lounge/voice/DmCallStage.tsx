import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { Avatar, Button } from '@sideroom/ui';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { RING_INTERVAL_MS, useCalls } from '../../voice/calls';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { useCurrentUser, useUsers, useVoiceMembers } from '../data';
import { CallControls, type ViewLayout } from './CallControls';
import { ParticipantTile } from './Tiles';
import './voice-p10.css';
import './voice-p13.css';

export const STAGE_HEIGHT_KEY = 'sr.dmcall.height';
export const STAGE_MIN = 160;
export const STAGE_DEFAULT = 280;

const clampHeight = (h: number, max: number) => Math.min(max, Math.max(STAGE_MIN, Math.round(h)));
const maxHeight = () => Math.max(STAGE_MIN, Math.round(globalThis.innerHeight * 0.7) || 480);

function savedHeight(): number {
  try {
    const n = Number(localStorage.getItem(STAGE_HEIGHT_KEY));
    return Number.isFinite(n) && n > 0 ? n : STAGE_DEFAULT;
  } catch {
    return STAGE_DEFAULT;
  }
}

/** The user ids of a call that are still ringing and not in it yet. */
export function useRinging(channelId: string, joined: string[]): string[] {
  const ringing = useCalls((s) => s.ringing[channelId]);
  return (ringing ?? []).filter((id) => !joined.includes(id));
}

/** Tell the server to stop ringing everyone who has not answered (the caller gives up). */
export const stopRinging = (channelId: string) =>
  apiFetch<unknown>(`/channels/${encodeURIComponent(channelId)}/call/stop-ringing`, {
    method: 'POST',
    json: {},
    retries: 0,
  }).catch(() => undefined);

/**
 * The call stage of a DM or group DM (VOX-UI-011): the header area of the conversation grows into the call,
 * with the people in it as tiles, the call controls, an outgoing "Calling ..." state for everyone who is still
 * ringing (VOX-UI-010) and a handle to drag the stage taller or shorter (arrow keys work too). The messages
 * stay below.
 */
export function DmCallStage({ channelId }: { channelId: string }) {
  const t = useT();
  const session = useVoiceSession();
  const me = useCurrentUser();
  const users = useUsers();
  const members = useVoiceMembers(channelId);
  const ringing = useRinging(
    channelId,
    members.map((m) => m.userId),
  );
  const [height, setHeight] = useState(() => clampHeight(savedHeight(), maxHeight()));
  const [layout, setLayout] = useState<ViewLayout>({
    mode: 'grid',
    hideNonVideo: false,
    chatOpen: false,
  });
  const [error, setError] = useState<string | null>(null);
  const alone = members.filter((m) => m.userId !== me.id).length === 0;
  const nameOf = (id: string) => users[id]?.displayName ?? t('lounge.voiceChannel.unknown');

  const resize = (h: number) => {
    const next = clampHeight(h, maxHeight());
    setHeight(next);
    try {
      localStorage.setItem(STAGE_HEIGHT_KEY, String(next));
    } catch {
      /* private mode: the size lasts until the page closes */
    }
  };
  const dragRef = useRef({ from: 0, start: 0, on: false });
  const stageRef = useRef<HTMLDivElement | null>(null);
  const down = (e: PointerEvent<HTMLDivElement>) => {
    dragRef.current = { on: true, from: e.clientY, start: height };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (d.on) resize(d.start + (e.clientY - d.from));
  };
  const up = () => {
    dragRef.current.on = false;
  };
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'ArrowDown') resize(height + 16);
    else if (e.key === 'ArrowUp') resize(height - 16);
    else if (e.key === 'Home') resize(STAGE_MIN);
    else if (e.key === 'End') resize(maxHeight());
    else return;
    e.preventDefault();
  };

  // A ringback tone while the caller waits alone for someone to pick up.
  const waiting = ringing.length > 0 && alone && session.status === 'connected';
  useEffect(() => {
    if (!waiting) return;
    voiceController().playRing();
    const id = setInterval(() => voiceController().playRing(), RING_INTERVAL_MS);
    return () => clearInterval(id);
  }, [waiting]);

  return (
    <div className="vc-dmcall" data-testid="dm-call-stage">
      <div className="vc-dmcall-stage" ref={stageRef} style={{ blockSize: height }}>
        {error && (
          <p role="alert" className="vc-inline-error">
            {error}
          </p>
        )}
        <ul className="vc-grid vc-dmcall-grid" aria-label={t('lounge.dmCall.people')}>
          {members.map((p) => (
            <ParticipantTile
              key={p.userId}
              guildId={DM_GUILD_ID}
              channelId={channelId}
              participant={p}
              onError={setError}
            />
          ))}
          {ringing.map((id) => (
            <li key={`ring:${id}`} className="vc-tile-item">
              <div
                className="vc-tile vc-tile-ringing"
                data-testid={`ringing-${id}`}
                role="group"
                aria-label={t('lounge.dmCall.calling', { name: nameOf(id) })}
              >
                <Avatar name={nameOf(id)} src={users[id]?.avatarUrl ?? null} size={80} />
                <div className="vc-tile-name">
                  <span>{nameOf(id)}</span>
                </div>
                <span className="vc-ringing-label" role="status">
                  {t('lounge.dmCall.callingShort')}
                </span>
              </div>
            </li>
          ))}
        </ul>
        {waiting && (
          <div className="vc-dmcall-waiting">
            <Button size="sm" variant="secondary" onClick={() => void stopRinging(channelId)}>
              {t('lounge.dmCall.cancel')}
            </Button>
          </div>
        )}
        <CallControls
          channelId={channelId}
          guildId={DM_GUILD_ID}
          layout={layout}
          onLayout={(patch) => setLayout((l) => ({ ...l, ...patch }))}
          hideChat
          onFullscreen={() => {
            void stageRef.current
              ?.requestFullscreen?.()
              .catch(() => setError(t('lounge.voiceStream.noFullscreen')));
          }}
        />
      </div>
      {/* A window splitter: drag, or Arrow Up/Down, Home and End. */}
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex, jsx-a11y/no-noninteractive-element-interactions */}
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label={t('lounge.dmCall.resize')}
        aria-valuenow={height}
        aria-valuemin={STAGE_MIN}
        aria-valuemax={maxHeight()}
        tabIndex={0}
        className="vc-dmcall-resize"
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        onKeyDown={key}
      />
    </div>
  );
}
