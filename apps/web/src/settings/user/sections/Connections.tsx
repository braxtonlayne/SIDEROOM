import { getOwnProfile, useResource, type OwnProfile, type Resource } from '../api';
import { Card, Loaded, SectionTitle, Unavailable } from '../parts';

function DiscordCard({ profile }: { profile: Resource<OwnProfile> }) {
  const link = profile.data?.connected_accounts.find((c) => c.type === 'discord');
  return (
    <Card id="us-discord" title="Linked Discord account">
      <Loaded res={profile} what="connections">
        {() =>
          link ? (
            <p>
              Linked as <strong>{link.name}</strong>
              <span className="us-badge" data-tone={link.verified ? 'good' : undefined}>
                {link.verified ? 'Verified' : 'Unverified'}
              </span>
            </p>
          ) : (
            <p className="us-muted">No Discord account is linked.</p>
          )
        }
      </Loaded>
      <p className="us-muted">
        Linking or unlinking from settings isn&apos;t available yet; Discord is linked while signing
        in.
      </p>
    </Card>
  );
}

const PROVIDERS = ['Discord', 'GitHub', 'Spotify', 'Steam', 'Twitch', 'YouTube', 'Reddit'];

export function ConnectionsSection() {
  const profile = useResource(getOwnProfile);
  const others = profile.data?.connected_accounts.filter((c) => c.type !== 'discord') ?? [];
  return (
    <>
      <SectionTitle>Connections</SectionTitle>
      <DiscordCard profile={profile} />
      {others.length > 0 && (
        <Card id="us-connected" title="Connected accounts">
          <ul className="us-list">
            {others.map((c) => (
              <li key={c.type + c.id}>
                <div className="us-row-main">
                  <strong>{c.name}</strong> <span className="us-muted">{c.type}</span>
                  <span className="us-badge" data-tone={c.verified ? 'good' : undefined}>
                    {c.verified ? 'Verified' : 'Unverified'}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
      <Card
        id="us-add-connection"
        title="Add a connection"
        description="Show the accounts you use on your profile."
      >
        <ul className="us-providers" aria-label="Providers">
          {PROVIDERS.map((p) => (
            <li key={p}>
              <span>{p}</span>
            </li>
          ))}
        </ul>
        <Unavailable>
          The server can sign you in with Discord but has no routes to link, unlink or refresh
          connections for a signed-in account, so none of these can be added yet.
        </Unavailable>
      </Card>
    </>
  );
}
