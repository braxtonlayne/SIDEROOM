// ARC-GST-002 (D-385, D-397): the page behind `/play/<lobby_token>#key=<game key>`. It is the second surface a person
// without a login can load beyond the entrance, and it is deliberately small: read the link, join the lobby, open
// the lobby's one socket and run the game in the same sandboxed frame members use, behind the reduced `guest`
// bridge. The relay gives the guest a random callsign (guests never choose a name). Nothing here imports the app.
import { ThemeProvider } from '@sideroom/ui';
import type { Frame } from '@sideroom/embedded-app-sdk';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n/react';
import { runnerRefusal } from './capabilities';
import type { GuestJoined } from './contract';
import { createGuestHost } from './guestHost';
import { guestLane, type GuestLaneKit } from './lane';
import { parseGuestLocation } from './link';
import { importGameKey } from './seal';
import {
  guestSocketUrl,
  joinLobby,
  JoinError,
  openLobby,
  type JoinFailure,
  type Lobby,
} from './session';
import './guest.css';

/** Same string as `ACTIVITY_SANDBOX` of the member frame; `guest.test.tsx` pins them together. */
export const GUEST_SANDBOX = 'allow-scripts allow-pointer-lock';

/** An application id is a number; anything else could smuggle a host into the frame's address. */
const APP_ID = /^\d{1,20}$/;

/** `https://<application-id>.activities.<host>/`, the origin the game's bundle is served from. */
export function gameFrameUrl(
  applicationId: string,
  loc: Pick<Location, 'protocol' | 'host'> = window.location,
): string | null {
  return APP_ID.test(applicationId)
    ? `${loc.protocol}//${applicationId}.activities.${loc.host}/`
    : null;
}

type Phase =
  | { k: 'ask' }
  | { k: 'joining' }
  | { k: 'playing'; join: GuestJoined; lobby: Lobby; kit: GuestLaneKit; seat: number }
  | { k: 'failed'; why: 'invalid' | 'nokey' | JoinFailure }
  | { k: 'refused'; why: 'ingame_voice' | 'not_guest_play' }
  | { k: 'ended' };

export interface RunnerDeps {
  loc?: Pick<Location, 'pathname' | 'hash' | 'protocol' | 'host'>;
  join?: typeof joinLobby;
  open?: typeof openLobby;
  /** How long to wait for the member bridge to name the host before the game starts anyway. */
  hostWaitMs?: number;
}

export function Runner({
  loc,
  join = joinLobby,
  open = openLobby,
  hostWaitMs = 1500,
}: RunnerDeps = {}) {
  const t = useT();
  const where = loc ?? window.location;
  const link = useMemo(
    () => parseGuestLocation(where.pathname, where.hash),
    [where.pathname, where.hash],
  );
  const [phase, setPhase] = useState<Phase>(() =>
    !link
      ? { k: 'failed', why: 'invalid' }
      : !link.key
        ? { k: 'failed', why: 'nokey' }
        : { k: 'ask' },
  );
  const lobbyRef = useRef<Lobby | null>(null);
  const [players, setPlayers] = useState(0);

  const start = useCallback(
    async (e?: React.FormEvent) => {
      e?.preventDefault();
      if (!link?.key) return;
      setPhase({ k: 'joining' });
      lobbyRef.current?.close();
      try {
        const key = await importGameKey(link.key);
        const joined = await join(link.token);
        // The runner refuses what a guest must not run before it opens the lobby's socket (D-397, D-399).
        const refusal = runnerRefusal(joined.capabilities);
        if (refusal) {
          setPhase({ k: 'refused', why: refusal });
          return;
        }
        const lobby = open({
          url: guestSocketUrl(where),
          hello: { guest_token: joined.guest_token },
          lobbyToken: link.token,
          key,
        });
        lobbyRef.current = lobby;
        const seated = await lobby.seated;
        const kit = guestLane(lobby);
        lobby.onPeers(() => setPlayers(lobby.peers().length));
        lobby.onEnd(() => setPhase((p) => (p.k === 'playing' ? { k: 'ended' } : p)));
        setPlayers(lobby.peers().length);
        // The member bridge tells the lobby who the game's host is right after the seat opens.
        await kit.hostKnown(hostWaitMs);
        setPhase({ k: 'playing', join: joined, lobby, kit, seat: seated.seat });
      } catch (err) {
        lobbyRef.current?.close();
        setPhase({ k: 'failed', why: err instanceof JoinError ? err.reason : 'network' });
      }
    },
    [link, join, open, where, hostWaitMs],
  );

  useEffect(
    () => () => {
      lobbyRef.current?.close();
      lobbyRef.current = null;
    },
    [],
  );

  return (
    <ThemeProvider section="entrance" platformDefault="sideroom" className="gp-root">
      <main className="gp-main" aria-live="polite">
        {phase.k === 'ask' && (
          <form className="gp-card" onSubmit={(e) => void start(e)}>
            <h1>{t('guestplay.page.title')}</h1>
            <p>{t('guestplay.page.intro')}</p>
            <button type="submit" className="gp-btn">
              {t('guestplay.join')}
            </button>
          </form>
        )}
        {phase.k === 'joining' && <p className="gp-card">{t('guestplay.joining')}</p>}
        {phase.k === 'failed' && (
          <div className="gp-card" role="alert">
            <p>{t(`guestplay.error.${phase.why === 'rate_limited' ? 'rate' : phase.why}`)}</p>
            {phase.why !== 'invalid' && phase.why !== 'nokey' && phase.why !== 'gone' && (
              <button type="button" className="gp-btn" onClick={() => setPhase({ k: 'ask' })}>
                {t('guestplay.retry')}
              </button>
            )}
          </div>
        )}
        {phase.k === 'refused' && (
          <div className="gp-card" role="alert">
            <p>
              {phase.why === 'ingame_voice'
                ? t('guestplay.refused.voice')
                : t('guestplay.refused.notGuest')}
            </p>
          </div>
        )}
        {phase.k === 'ended' && (
          <div className="gp-card" role="alert">
            <h1>{t('guestplay.ended.title')}</h1>
            <p>{t('guestplay.ended.body')}</p>
            <button type="button" className="gp-btn" onClick={() => setPhase({ k: 'ask' })}>
              {t('guestplay.retry')}
            </button>
          </div>
        )}
        {phase.k === 'playing' && (
          <Playing
            join={phase.join}
            lobby={phase.lobby}
            kit={phase.kit}
            seat={phase.seat}
            players={players}
            loc={where}
          />
        )}
      </main>
    </ThemeProvider>
  );
}

function Playing({
  join,
  lobby,
  kit,
  seat,
  players,
  loc,
}: {
  join: GuestJoined;
  lobby: Lobby;
  kit: GuestLaneKit;
  seat: number;
  players: number;
  loc: Pick<Location, 'protocol' | 'host'>;
}) {
  const t = useT();
  const frame = useRef<HTMLIFrameElement | null>(null);
  const src = gameFrameUrl(join.application_id, loc);

  const host = useMemo(
    () =>
      createGuestHost({
        applicationId: join.application_id,
        declared: join.capabilities,
        self: () => ({ id: `g${seat}`, callsign: join.callsign }),
        roster: kit.roster,
        hostId: kit.hostId,
        onHostChange: kit.onHost,
        lane: kit.lane,
        // An opaque-origin frame cannot be named as a target origin: the window itself is the address.
        post: (f: Frame) => frame.current?.contentWindow?.postMessage(f, '*'),
      }),
    [join, kit, seat],
  );

  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      void host.handle(e.data, e.origin);
    };
    window.addEventListener('message', onMessage);
    const off = lobby.onPeers(() => host.rosterChanged());
    return () => {
      window.removeEventListener('message', onMessage);
      off();
      host.dispose();
    };
  }, [host, lobby]);

  if (!src) return null;
  return (
    <section className="gp-play">
      <header className="gp-bar">
        <strong>{join.application_name}</strong>
        <span className="gp-muted">
          {t('guestplay.playing.as', { name: join.callsign })}
          {players > 0 ? ` · ${t('guestplay.playing.players', { n: players })}` : ''}
        </span>
      </header>
      <iframe
        ref={frame}
        className="gp-frame"
        title={t('guestplay.frame.title', { name: join.application_name })}
        src={src}
        sandbox={GUEST_SANDBOX}
        allow="fullscreen"
        referrerPolicy="no-referrer"
      />
    </section>
  );
}
