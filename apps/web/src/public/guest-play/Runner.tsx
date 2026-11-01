// ARC-GST-002 (D-385, D-397): the page behind `/play/<lobby_token>#key=<game key>`. It is the second surface a person
// without a login can load beyond the entrance, and it is deliberately small: read the link, ask for a nickname,
// join the lobby, open the lobby's one socket and run the game in the same sandboxed frame members use, behind the
// reduced `guest` bridge. Nothing here imports the app.
import { ThemeProvider } from '@sideroom/ui';
import type { Frame } from '@sideroom/embedded-app-sdk';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n/react';
import { cleanNickname, NICKNAME_MAX, randomCallsign } from './callsign';
import { runnerRefusal } from './capabilities';
import type { GuestJoinResponse } from './contract';
import { createGuestHost } from './guestHost';
import { parseGuestLocation } from './link';
import { importGameKey } from './seal';
import {
  guestSocketUrl,
  joinLobby,
  JoinError,
  openGuestSocket,
  type GuestSocket,
  type JoinFailure,
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
  | { k: 'playing'; join: GuestJoinResponse }
  | { k: 'failed'; why: 'invalid' | 'nokey' | JoinFailure }
  | { k: 'refused'; why: 'ingame_voice' | 'not_guest_play' }
  | { k: 'ended' };

export interface RunnerDeps {
  loc?: Pick<Location, 'pathname' | 'hash' | 'protocol' | 'host'>;
  join?: typeof joinLobby;
  open?: typeof openGuestSocket;
}

export function Runner({ loc, join = joinLobby, open = openGuestSocket }: RunnerDeps = {}) {
  const t = useT();
  const where = loc ?? window.location;
  const link = useMemo(() => parseGuestLocation(where.pathname, where.hash), [where]);
  const callsign = useMemo(() => randomCallsign(), []);
  const [nick, setNick] = useState('');
  const [nickBad, setNickBad] = useState(false);
  const [phase, setPhase] = useState<Phase>(() =>
    !link ? { k: 'failed', why: 'invalid' } : !link.key ? { k: 'failed', why: 'nokey' } : { k: 'ask' },
  );
  const socketRef = useRef<{ socket: GuestSocket; join: GuestJoinResponse } | null>(null);
  const [players, setPlayers] = useState(0);

  const start = useCallback(
    async (e?: React.FormEvent) => {
      e?.preventDefault();
      if (!link?.key) return;
      const wanted = nick.trim() === '' ? null : cleanNickname(nick);
      if (nick.trim() !== '' && !wanted) {
        setNickBad(true);
        return;
      }
      setNickBad(false);
      setPhase({ k: 'joining' });
      try {
        const key = await importGameKey(link.key);
        const joined = await join(link.token, {
          callsign,
          ...(wanted ? { nickname: wanted } : {}),
        });
        // The runner refuses what a guest must not run before it opens the lobby's socket (D-397, D-399).
        const refusal = runnerRefusal(joined.capabilities);
        if (refusal) {
          setPhase({ k: 'refused', why: refusal });
          return;
        }
        const socket = open({
          url: guestSocketUrl(where),
          sessionToken: joined.session_token,
          lobbyToken: link.token,
          key,
        });
        socketRef.current = { socket, join: joined };
        socket.onEnd(() => setPhase((p) => (p.k === 'playing' ? { k: 'ended' } : p)));
        socket.onRoster(() => setPlayers(socket.roster().length));
        setPhase({ k: 'playing', join: joined });
      } catch (err) {
        setPhase({ k: 'failed', why: err instanceof JoinError ? err.reason : 'network' });
      }
    },
    [link, nick, callsign, join, open, where],
  );

  useEffect(
    () => () => {
      socketRef.current?.socket.close();
      socketRef.current = null;
    },
    [],
  );

  return (
    <ThemeProvider section="entrance" platformDefault="sideroom" className="gp-root">
      <main className="gp-main" aria-live="polite">
        {phase.k === 'ask' && (
          <form className="gp-card" onSubmit={(e) => void start(e)}>
            <h1>{t('guestPlay.page.title')}</h1>
            <p>{t('guestPlay.page.intro')}</p>
            <label className="gp-field">
              <span>{t('guestPlay.nickname.label')}</span>
              <input
                value={nick}
                maxLength={NICKNAME_MAX * 2}
                autoComplete="off"
                spellCheck={false}
                placeholder={callsign}
                aria-invalid={nickBad}
                aria-describedby="gp-nick-hint"
                onChange={(e) => setNick(e.target.value)}
              />
            </label>
            <p id="gp-nick-hint" className="gp-muted">
              {nickBad ? t('guestPlay.nickname.invalid') : t('guestPlay.nickname.hint', { callsign })}
            </p>
            <button type="submit" className="gp-btn">
              {t('guestPlay.join')}
            </button>
          </form>
        )}
        {phase.k === 'joining' && <p className="gp-card">{t('guestPlay.joining')}</p>}
        {phase.k === 'failed' && (
          <div className="gp-card" role="alert">
            <p>{t(`guestPlay.error.${phase.why === 'rate_limited' ? 'rate' : phase.why}`)}</p>
            {phase.why !== 'invalid' && phase.why !== 'nokey' && phase.why !== 'gone' && (
              <button type="button" className="gp-btn" onClick={() => setPhase({ k: 'ask' })}>
                {t('guestPlay.retry')}
              </button>
            )}
          </div>
        )}
        {phase.k === 'refused' && (
          <div className="gp-card" role="alert">
            <p>
              {phase.why === 'ingame_voice'
                ? t('guestPlay.refused.voice')
                : t('guestPlay.refused.notGuest')}
            </p>
          </div>
        )}
        {phase.k === 'ended' && (
          <div className="gp-card" role="alert">
            <h1>{t('guestPlay.ended.title')}</h1>
            <p>{t('guestPlay.ended.body')}</p>
            <button type="button" className="gp-btn" onClick={() => setPhase({ k: 'ask' })}>
              {t('guestPlay.retry')}
            </button>
          </div>
        )}
        {phase.k === 'playing' && socketRef.current && (
          <Playing
            join={phase.join}
            socket={socketRef.current.socket}
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
  socket,
  players,
  loc,
}: {
  join: GuestJoinResponse;
  socket: GuestSocket;
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
        self: { id: join.guest_id, callsign: join.callsign },
        roster: () => socket.roster(),
        lane: socket.lane,
        // An opaque-origin frame cannot be named as a target origin: the window itself is the address.
        post: (f: Frame) => frame.current?.contentWindow?.postMessage(f, '*'),
      }),
    [join, socket],
  );

  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      void host.handle(e.data, e.origin);
    };
    window.addEventListener('message', onMessage);
    const off = socket.onRoster(() => host.rosterChanged());
    return () => {
      window.removeEventListener('message', onMessage);
      off();
      host.dispose();
    };
  }, [host, socket]);

  if (!src) return null;
  return (
    <section className="gp-play">
      <header className="gp-bar">
        <strong>{join.lobby_title || join.application_name}</strong>
        <span className="gp-muted">
          {t('guestPlay.playing.as', { name: join.callsign })}
          {players > 0 ? ` · ${t('guestPlay.playing.players', { n: players })}` : ''}
        </span>
      </header>
      <iframe
        ref={frame}
        className="gp-frame"
        title={t('guestPlay.frame.title', { name: join.application_name })}
        src={src}
        sandbox={GUEST_SANDBOX}
        allow="fullscreen"
        referrerPolicy="no-referrer"
      />
    </section>
  );
}
