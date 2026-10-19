// Server Settings > Watch Parties & Library (LNG-SSET-051). Who may start a watch party is the role permission
// "Start Watch Party"; the announcer channels and filters belong to the two community modules (watch party
// announcer, library feed), so this page puts those modules' switches and forms together in one place.
import { Notice } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useT } from '../../../i18n';
import * as api from '../featuresApi';
import { useResource } from '../hooks';
import { Async, SectionHeader } from '../ui';
import type { SectionProps } from '../ServerSettings';
import { FeatureCard, useFeaturePickers } from './Features';

export const WATCH_LIBRARY_FEATURE_IDS = [
  'community.watch_party_announcer',
  'community.library_feed',
] as const;

export function WatchLibrarySection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const res = useResource(`watchlib-${guildId}`, () => api.listFeatures(guildId));
  const pickers = useFeaturePickers(guildId);
  return (
    <>
      <SectionHeader
        title={t('settings.watchLibrary.title')}
        description={t('settings.watchLibrary.description')}
      />
      <article className="ss-card" aria-labelledby="wl-who-h">
        <h2 id="wl-who-h" className="ss-card-title">
          {t('settings.watchLibrary.whoTitle')}
        </h2>
        <p className="ss-desc">{t('settings.watchLibrary.whoBody')}</p>
        {viewer.can('MANAGE_ROLES') && (
          <Link to="/settings/server/$guildId/$section" params={{ guildId, section: 'roles' }}>
            {t('settings.watchLibrary.openRoles')}
          </Link>
        )}
      </article>
      <Async res={res} label={t('settings.features.loading')}>
        {(features) => {
          const mine = WATCH_LIBRARY_FEATURE_IDS.map((id) => features.find((f) => f.id === id)).filter(
            (f): f is api.GuildFeature => !!f,
          );
          if (mine.length === 0)
            return <Notice variant="info">{t('settings.watchLibrary.none')}</Notice>;
          return (
            <>
              {mine.map((f) => (
                <FeatureCard
                  key={f.id}
                  guildId={guildId}
                  feature={f}
                  pickers={pickers}
                  onChanged={(next) =>
                    res.set((cur) => (cur ?? []).map((x) => (x.id === next.id ? next : x)))
                  }
                />
              ))}
            </>
          );
        }}
      </Async>
    </>
  );
}
