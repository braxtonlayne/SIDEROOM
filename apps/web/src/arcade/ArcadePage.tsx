import { Button, EmptyState, Icon, Spinner } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useEmulatorOn } from '../emu/SwitchedOff';
import { useGuilds } from '../store/hooks';
import { apiFetch } from '../api/client';
import type { Guild } from '../store/types';
import { useShowroomActive } from '../showroom/state';
import { httpActivityApi, type LobbyApi } from '../activities/api';
import { useLobbies } from '../activities/hooks';
import type { Lobby } from '../activities/types';
import { ApiError } from '../api/client';
import type { JoinTarget } from '../voice/controller';
import { useVoiceSession } from '../voice/session';
import { httpArcadeApi, type ArcadeApi } from './api';
import { ArcadeShell } from './ArcadeShell';
import {
  ArcadeHero,
  CartridgesSection,
  ComingSoonSection,
  FeaturedSection,
  LobbiesSection,
  PlayableSection,
  type LobbyError,
} from './ArcadeSections';
import { sandboxTrust } from './sandboxTrust';
import { joinCall, StartInDialog, type PlayTarget } from './StartInDialog';
import { TrustProvider, type TrustSource } from './trust';
import type { ArcadeFront } from './types';
import './arcade.css';

/**
 * The Arcade (ARC-ROOM-001, redesigned in ARC-UI-010), route `/games`. Playable now is the built-in games and the
 * Emulator, drawn without waiting for data; Featured is the Operator's Front Page; Guild Cartridges are unlisted
 * games and mods a server lists; Coming Soon shows games in development. Loaded only after sign-in (D-385).
 */
// UX-SHOW-001: sample cards live in a lazy chunk and replace the page (not the data) while the preview is on.
const PreviewArcade = lazy(() => import('../showroom/PreviewArcade'));

/**
 * The member's servers: the live list once the Lounge has connected, else (the Arcade opened straight from a link or
 * a reload, before the gateway is up) the REST list, so the cartridges picker is never wrongly empty.
 */
function useServerList(): Pick<Guild, 'id' | 'name'>[] {
  const live = useGuilds();
  const [fetched, setFetched] = useState<Pick<Guild, 'id' | 'name'>[]>([]);
  const need = live.length === 0;
  useEffect(() => {
    if (!need) return;
    let on = true;
    apiFetch<{ id: string; name?: string }[]>('/users/@me/guilds', { retries: 0 })
      .then((g) => {
        if (on && Array.isArray(g)) setFetched(g.map((x) => ({ id: x.id, name: x.name ?? '' })));
      })
      .catch(() => undefined);
    return () => {
      on = false;
    };
  }, [need]);
  return need ? fetched : live;
}

/**
 * The Activities shelf by name (D-477): only read when the Operator has picks, so a pick that is the same game as a
 * built-in hero can take that hero's slot instead of showing twice.
 */
function useShelfIds(need: boolean): Record<string, string> {
  const [ids, setIds] = useState<Record<string, string>>({});
  useEffect(() => {
    if (!need) return;
    let on = true;
    httpActivityApi
      .shelf()
      .then((s) => on && setIds(Object.fromEntries(s.items.map((i) => [i.name, i.application_id]))))
      .catch(() => undefined);
    return () => {
      on = false;
    };
  }, [need]);
  return ids;
}

export function ArcadePage(props: {
  api?: ArcadeApi;
  trust?: TrustSource;
  lobbyApi?: LobbyApi;
  callJoin?: (target: JoinTarget) => Promise<boolean>;
}) {
  const preview = useShowroomActive();
  if (preview)
    return (
      <Suspense fallback={null}>
        <PreviewArcade />
      </Suspense>
    );
  return <RealArcadePage {...props} />;
}

/** How long "This game has ended." stays on its card before the strip is fetched again without it (D-486). */
export const ENDED_REFETCH_MS = 2500;

/**
 * Join from the Active lobbies strip (D-486), as Play does after a start (D-478): a voice lobby joins that call first
 * (a failed join stops there), then the instance (unless the member is in it already: Return), then the channel opens
 * with the Activity in its full-size slot. A 404 says the game ended and fetches the strip again; a 403 says it could
 * not be joined.
 */
function useLobbyJoin(
  api: LobbyApi,
  join: (t: JoinTarget) => Promise<boolean>,
  refetch: () => void,
) {
  const navigate = useNavigate();
  const [busy, setBusy] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, LobbyError>>({});
  const later = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (later.current) clearTimeout(later.current);
    },
    [],
  );
  const fail = (id: string, e: LobbyError) => setErrors((m) => ({ ...m, [id]: e }));
  const onJoin = async (l: Lobby) => {
    if (busy) return;
    setBusy(l.instance_id);
    setErrors((m) => {
      const rest = { ...m };
      delete rest[l.instance_id];
      return rest;
    });
    try {
      const s = useVoiceSession.getState();
      const inCall = s.status === 'connected' && s.channelId === l.channel_id;
      if (l.channel_type === 'voice' && !inCall) {
        const ok = await join({
          guildId: l.guild_id,
          channelId: l.channel_id,
          name: l.channel_name,
        });
        if (!ok) return fail(l.instance_id, 'call');
      }
      if (!l.joined) {
        try {
          await api.join(l.instance_id);
        } catch (e) {
          const status = e instanceof ApiError ? e.status : 0;
          if (status === 404) {
            fail(l.instance_id, 'ended');
            if (later.current) clearTimeout(later.current);
            later.current = setTimeout(refetch, ENDED_REFETCH_MS);
          } else {
            fail(l.instance_id, 'failed');
            if (status === 403) refetch();
          }
          return;
        }
      }
      void navigate({
        to: '/channels/$guildId/$channelId',
        params: { guildId: l.guild_id, channelId: l.channel_id },
      });
    } finally {
      setBusy(null);
    }
  };
  return { busy, errors, onJoin: (l: Lobby) => void onJoin(l) };
}

function RealArcadePage({
  api = httpArcadeApi,
  trust = sandboxTrust,
  lobbyApi = httpActivityApi,
  callJoin = joinCall,
}: {
  api?: ArcadeApi;
  /** The build-trust badges on the cards (ARC-SBX-001); tests pass their own. */
  trust?: TrustSource;
  /** The Active lobbies strip's list and join (ARC-LOB-001); tests pass their own. */
  lobbyApi?: LobbyApi;
  /** Joins a call and resolves once connected (false when it could not); tests pass their own. */
  callJoin?: (target: JoinTarget) => Promise<boolean>;
}) {
  const t = useT();
  const guilds = useServerList();
  const emulatorOn = useEmulatorOn();
  const [front, setFront] = useState<ArcadeFront | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [playing, setPlaying] = useState<PlayTarget | null>(null);
  const [listSignal, setListSignal] = useState(0);
  const shelfIds = useShelfIds(!!front && front.featured.length > 0);
  const lobbies = useLobbies(true, lobbyApi);
  const lobbyJoin = useLobbyJoin(lobbyApi, callJoin, lobbies.refetch);

  useEffect(() => {
    let live = true;
    setFailed(false);
    api
      .front()
      .then((f) => live && setFront(f))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [api, attempt]);

  const links = [
    // D-486: the lobbies link only while the strip has items (the D-474 pattern).
    ...(lobbies.items.length > 0 ? [{ href: '#lobbies', label: t('arcade.lobbies.heading') }] : []),
    { href: '#playable', label: t('arcade.playable.heading') },
    // D-474: no Featured link while there are no picks (the section is not on the page).
    ...(front && front.featured.length > 0
      ? [{ href: '#featured', label: t('arcade.featured.heading') }]
      : []),
    { href: '#cartridges', label: t('arcade.cartridges.heading') },
    { href: '#coming-soon', label: t('arcade.soon.heading') },
  ];

  return (
    <ArcadeShell
      links={links}
      action={
        guilds.length > 0 ? (
          <a
            href="#cartridges"
            className="arc-tabs-link"
            onClick={() => setListSignal((n) => n + 1)}
          >
            <Icon name="plus" size={16} />
            <span className="arc-tabs-label">{t('arcade.cartridges.listButton')}</span>
          </a>
        ) : undefined
      }
    >
      <ArcadeHero
        flags={front?.flags ?? []}
        onPlay={setPlaying}
        featured={front?.featured ?? []}
        shelfIds={shelfIds}
      />
      <LobbiesSection
        items={lobbies.items}
        onJoin={lobbyJoin.onJoin}
        busy={lobbyJoin.busy}
        errors={lobbyJoin.errors}
      />
      <PlayableSection
        flags={front?.flags ?? []}
        emulatorOn={emulatorOn}
        onPlay={setPlaying}
        lobbies={lobbies.items}
      />
      {failed && (
        <div className="arc-section">
          <EmptyState
            className="arc-empty"
            data-tone="danger"
            role="alert"
            icon={<Icon name="alert" size={32} />}
            title={t('arcade.page.failed')}
            action={
              <Button variant="secondary" onClick={() => setAttempt((n) => n + 1)}>
                {t('arcade.page.retry')}
              </Button>
            }
          />
        </div>
      )}
      {!front && !failed && (
        <div className="arc-section arc-loading" aria-busy="true">
          <Spinner label={t('arcade.page.loading')} />
          <div className="arc-grid" aria-hidden="true">
            <div className="arc-skeleton" />
            <div className="arc-skeleton" />
            <div className="arc-skeleton" />
          </div>
        </div>
      )}
      {front && (
        <TrustProvider source={trust}>
          <FeaturedSection
            featured={front.featured}
            flags={front.flags}
            onPlay={setPlaying}
            lobbies={lobbies.items}
          />
          <CartridgesSection
            guilds={guilds}
            flags={front.flags}
            api={api}
            openSignal={listSignal}
          />
          <ComingSoonSection entries={front.coming_soon} api={api} flags={front.flags} />
        </TrustProvider>
      )}
      <StartInDialog target={playing} guilds={guilds} onClose={() => setPlaying(null)} />
    </ArcadeShell>
  );
}
