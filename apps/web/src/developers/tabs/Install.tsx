import { Notice } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { isUnavailable, keys, listInstalls } from '../api';
import type { DevApp } from '../types';
import { fmtDate } from '../format';
import { LoadError, Loading } from '../ui';
import { UrlGenerator } from './OAuth2';

function Installs({ app }: { app: DevApp }) {
  const q = useQuery({ queryKey: keys.installs(app.id), queryFn: () => listInstalls(app.id) });
  return (
    <section className="dev-card" aria-labelledby="dev-installs-h">
      <h2 id="dev-installs-h" className="dev-card-title">
        Installed in
      </h2>
      {q.isPending ? (
        <Loading what="installs" />
      ) : q.error && isUnavailable(q.error) ? (
        <p className="dev-muted">Install records are not available on this server.</p>
      ) : q.error ? (
        <LoadError what="installs" error={q.error} onRetry={() => void q.refetch()} />
      ) : q.data.length === 0 ? (
        <p className="dev-muted">Not installed in any server yet.</p>
      ) : (
        <ul className="dev-uri-list" aria-label="Installs">
          {q.data.map((i) => (
            <li key={i.guild_id}>
              <span>
                Server <code>{i.guild_id}</code>
              </span>
              <span className="dev-muted">Added {fmtDate(i.created_at)}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function InstallTab({ app }: { app: DevApp }) {
  return (
    <div className="dev-stack">
      {app.release_state !== 'released' && (
        <Notice variant="info" title="Test servers only, for now">
          Until the app is released, only you can add it, to servers you manage that have 25 members
          or fewer. Anywhere else the install is refused with error 900120 (app not released).
          Submit a version in the Review tab to release it.
        </Notice>
      )}
      {!app.bot_public && (
        <Notice variant="info" title="Private bot">
          The bot is not public, so only you can add it. Turn on Public bot in General to let anyone
          with Manage Server add it.
        </Notice>
      )}
      <UrlGenerator app={app} installOnly />
      <Installs app={app} />
    </div>
  );
}
