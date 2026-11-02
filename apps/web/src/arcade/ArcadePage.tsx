import { Notice, Spinner } from '@sideroom/ui';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useT } from '../i18n';
import { useEmulatorOn } from '../emu/SwitchedOff';
import { useGuilds } from '../store/hooks';
import { PreviewToggle } from '../showroom/PreviewToggle';
import { useShowroomActive } from '../showroom/state';
import { httpArcadeApi, type ArcadeApi } from './api';
import { CartridgesSection, ComingSoonSection, FeaturedSection } from './ArcadeSections';
import { sandboxTrust } from './sandboxTrust';
import { TrustProvider, type TrustSource } from './trust';
import type { ArcadeFront } from './types';
import './arcade.css';

/**
 * The Arcade (ARC-ROOM-001), route `/games`. Featured is the Operator's Front Page; Guild Cartridges are unlisted
 * games and mods a server lists; Coming Soon shows games in development. Loaded only after sign-in (D-385).
 */
// UX-SHOW-001: sample cards live in a lazy chunk and replace the page (not the data) while the preview is on.
const PreviewArcade = lazy(() => import('../showroom/PreviewArcade'));

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
  const guilds = useGuilds();
  const emulatorOn = useEmulatorOn();
  const [front, setFront] = useState<ArcadeFront | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    api
      .front()
      .then((f) => live && setFront(f))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [api]);

  return (
    <main className="arc-page">
      <h1>{t('arcade.page.title')}</h1>
      <p className="arc-dim">{t('arcade.page.intro')}</p>
      <PreviewToggle />
      <nav aria-label={t('arcade.page.nav')} className="arc-nav">
        <a href="#featured">{t('arcade.featured.heading')}</a>
        <a href="#cartridges">{t('arcade.cartridges.heading')}</a>
        <a href="#coming-soon">{t('arcade.soon.heading')}</a>
      </nav>
      {failed && <Notice variant="danger" title={t('arcade.page.failed')} />}
      {!front && !failed && <Spinner label={t('arcade.page.loading')} />}
      {front && (
        <TrustProvider source={trust}>
          <FeaturedSection featured={front.featured} flags={front.flags} emulatorOn={emulatorOn} />
          <CartridgesSection guilds={guilds} flags={front.flags} api={api} />
          <ComingSoonSection entries={front.coming_soon} api={api} />
        </TrustProvider>
      )}
    </main>
  );
}
