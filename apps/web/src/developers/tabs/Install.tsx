import { Notice } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { isUnavailable, keys, listInstalls } from '../api';
import { useT } from '../../i18n';
import type { DevApp } from '../types';
import { fmtDate } from '../format';
import { LoadError, Loading } from '../ui';
import { UrlGenerator } from './OAuth2';
import { TestersCard } from './Testers';

function Installs({ app }: { app: DevApp }) {
  const t = useT();
  const q = useQuery({ queryKey: keys.installs(app.id), queryFn: () => listInstalls(app.id) });
  return (
    <section className="dev-card" aria-labelledby="dev-installs-h">
      <h2 id="dev-installs-h" className="dev-card-title">
        {t('developers.install.installedIn')}
      </h2>
      {q.isPending ? (
        <Loading what={t('developers.install.installs')} />
      ) : q.error && isUnavailable(q.error) ? (
        <p className="dev-muted">{t('developers.install.recordsUnavailable')}</p>
      ) : q.error ? (
        <LoadError
          what={t('developers.install.installs')}
          error={q.error}
          onRetry={() => void q.refetch()}
        />
      ) : q.data.length === 0 ? (
        <p className="dev-muted">{t('developers.install.none')}</p>
      ) : (
        <ul className="dev-uri-list" aria-label={t('developers.install.listLabel')}>
          {q.data.map((i) => (
            <li key={i.guild_id}>
              <span>
                {t('developers.install.server')} <code>{i.guild_id}</code>
              </span>
              <span className="dev-muted">
                {t('developers.install.added', { date: fmtDate(i.created_at) })}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function InstallTab({ app }: { app: DevApp }) {
  const t = useT();
  return (
    <div className="dev-stack">
      {app.release_state !== 'released' && (
        <Notice variant="info" title={t('developers.install.testOnlyTitle')}>
          {t('developers.install.testOnlyBody')}
        </Notice>
      )}
      {!app.bot_public && (
        <Notice variant="info" title={t('developers.install.privateTitle')}>
          {t('developers.install.privateBody')}
        </Notice>
      )}
      <UrlGenerator app={app} installOnly />
      <TestersCard app={app} />
      <Installs app={app} />
    </div>
  );
}
