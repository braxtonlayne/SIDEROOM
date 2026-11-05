import { Button, EmptyState, Icon, Spinner } from '@sideroom/ui';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useT } from '../i18n';
import { useEmulatorOn } from '../emu/SwitchedOff';
import { useGuilds } from '../store/hooks';
import { apiFetch } from '../api/client';
import type { Guild } from '../store/types';
import { PreviewToggle } from '../showroom/PreviewToggle';
import { useShowroomActive } from '../showroom/state';
import { httpArcadeApi, type ArcadeApi } from './api';
import { ArcadeHero } from './ArcadeHero';
import {
  CartridgesSection,
  ComingSoonSection,
  FeaturedSection,
  PlayableSection,
} from './ArcadeSections';
import { sandboxTrust } from './sandboxTrust';
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

export function ArcadePage(props: { api?: ArcadeApi; trust?: TrustSource }) {
  const preview = useShowroomActive();
  if (preview)
    return (
      <Suspense fallback={null}>
        <PreviewArcade />
      </Suspense>
    );
  return <RealArcadePage {...props} />;
}

function RealArcadePage({
  api = httpArcadeApi,
  trust = sandboxTrust,
}: {
  api?: ArcadeApi;
  /** The build-trust badges on the cards (ARC-SBX-001); tests pass their own. */
  trust?: TrustSource;
}) {
  const t = useT();
  const guilds = useServerList();
  const emulatorOn = useEmulatorOn();
  const [front, setFront] = useState<ArcadeFront | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

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
    { href: '#playable', label: t('arcade.playable.heading') },
    { href: '#featured', label: t('arcade.featured.heading') },
    { href: '#cartridges', label: t('arcade.cartridges.heading') },
    { href: '#coming-soon', label: t('arcade.soon.heading') },
  ];

  return (
    <main className="arc-page">
      <ArcadeHero links={links}>
        <PreviewToggle />
      </ArcadeHero>
      <PlayableSection flags={front?.flags ?? []} emulatorOn={emulatorOn} />
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
          <FeaturedSection featured={front.featured} flags={front.flags} />
          <CartridgesSection guilds={guilds} flags={front.flags} api={api} />
          <ComingSoonSection entries={front.coming_soon} api={api} flags={front.flags} />
        </TrustProvider>
      )}
    </main>
  );
}
