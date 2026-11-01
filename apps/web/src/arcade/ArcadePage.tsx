import { Notice, Spinner } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { useEmulatorOn } from '../emu/SwitchedOff';
import { useGuilds } from '../store/hooks';
import { httpArcadeApi, type ArcadeApi } from './api';
import { CartridgesSection, ComingSoonSection, FeaturedSection } from './ArcadeSections';
import type { ArcadeFront } from './types';
import './arcade.css';

/**
 * The Arcade (ARC-ROOM-001), route `/games`. Featured is the Operator's Front Page; Guild Cartridges are unlisted
 * games and mods a server lists; Coming Soon shows games in development. Loaded only after sign-in (D-385).
 */
export function ArcadePage({ api = httpArcadeApi }: { api?: ArcadeApi }) {
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
      <nav aria-label={t('arcade.page.nav')} className="arc-nav">
        <a href="#featured">{t('arcade.featured.heading')}</a>
        <a href="#cartridges">{t('arcade.cartridges.heading')}</a>
        <a href="#coming-soon">{t('arcade.soon.heading')}</a>
      </nav>
      {failed && <Notice variant="danger" title={t('arcade.page.failed')} />}
      {!front && !failed && <Spinner label={t('arcade.page.loading')} />}
      {front && (
        <>
          <FeaturedSection
            featured={front.featured}
            flags={front.flags}
            emulatorOn={emulatorOn}
          />
          <CartridgesSection guilds={guilds} flags={front.flags} api={api} />
          <ComingSoonSection entries={front.coming_soon} api={api} />
        </>
      )}
    </main>
  );
}
