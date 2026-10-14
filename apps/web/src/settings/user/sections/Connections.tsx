import { useT } from '../../../i18n';
import { getOwnProfile, useResource, type OwnProfile, type Resource } from '../api';
import { Card, Loaded, SectionTitle, Unavailable } from '../parts';

function DiscordCard({ profile }: { profile: Resource<OwnProfile> }) {
  const t = useT();
  const link = profile.data?.connected_accounts.find((c) => c.type === 'discord');
  return (
    <Card id="us-discord" title={t('settings.connections.discordTitle')}>
      <Loaded res={profile} what={t('settings.connections.what')}>
        {() =>
          link ? (
            <p>
              {t('settings.connections.linkedAs')} <strong>{link.name}</strong>
              <span className="us-badge" data-tone={link.verified ? 'good' : undefined}>
                {link.verified
                  ? t('settings.connections.verified')
                  : t('settings.connections.unverified')}
              </span>
            </p>
          ) : (
            <p className="us-muted">{t('settings.connections.noDiscord')}</p>
          )
        }
      </Loaded>
      <p className="us-muted">{t('settings.connections.linkingUnavailable')}</p>
    </Card>
  );
}

const PROVIDERS = ['Discord', 'GitHub', 'Spotify', 'Steam', 'Twitch', 'YouTube', 'Reddit'];

export function ConnectionsSection() {
  const t = useT();
  const profile = useResource(getOwnProfile);
  const others = profile.data?.connected_accounts.filter((c) => c.type !== 'discord') ?? [];
  return (
    <>
      <SectionTitle>{t('settings.connections.title')}</SectionTitle>
      <DiscordCard profile={profile} />
      {others.length > 0 && (
        <Card id="us-connected" title={t('settings.connections.connected')}>
          <ul className="us-list">
            {others.map((c) => (
              <li key={c.type + c.id}>
                <div className="us-row-main">
                  <strong>{c.name}</strong> <span className="us-muted">{c.type}</span>
                  <span className="us-badge" data-tone={c.verified ? 'good' : undefined}>
                    {c.verified
                      ? t('settings.connections.verified')
                      : t('settings.connections.unverified')}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
      <Card
        id="us-add-connection"
        title={t('settings.connections.addTitle')}
        description={t('settings.connections.addDesc')}
      >
        <ul className="us-providers" aria-label={t('settings.connections.providers')}>
          {PROVIDERS.map((p) => (
            <li key={p}>
              <span>{p}</span>
            </li>
          ))}
        </ul>
        <Unavailable>{t('settings.connections.addUnavailable')}</Unavailable>
      </Card>
    </>
  );
}
