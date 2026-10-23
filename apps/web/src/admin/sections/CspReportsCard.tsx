import { Badge, Button } from '@sideroom/ui';
import { useId } from 'react';
import * as api from '../api';
import { formatNumber, useT } from '../../i18n';
import { Async, fmtMs, useResource } from '../ui';

/**
 * Admin > System "Blocked content reports" card (RDY-SEC-009): the counters behind `POST /internal/csp-report`.
 * Directives, the top blocked origins and the pages they happened on; the server keeps nothing else.
 */
export function CspReportsCard() {
  const t = useT();
  const id = useId();
  const res = useResource('admin:csp-reports', api.getCspReports);
  const group = (title: string, rows: api.CspCounter[]) => (
    <div>
      <h3 className="ss-meta">{title}</h3>
      <ul className="ss-list" aria-label={title}>
        {rows.map((r) => (
          <li key={r.key} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">{r.key}</div>
              <div className="ss-meta">
                {t('admin.csp.lastSeen', { at: fmtMs(r.last_seen_ms) })}
              </div>
            </div>
            <Badge variant="neutral">
              {t('admin.csp.count', { count: formatNumber(r.count) })}
            </Badge>
          </li>
        ))}
      </ul>
    </div>
  );
  return (
    <section className="ss-card" aria-labelledby={id}>
      <h2 id={id}>{t('admin.csp.title')}</h2>
      <p className="ss-sub">{t('admin.csp.description')}</p>
      <div className="ss-toolbar">
        <Button variant="secondary" onClick={() => void res.reload()}>
          {t('admin.csp.refresh')}
        </Button>
      </div>
      <Async res={res} label={t('admin.system.loading')}>
        {(s) =>
          s.directives.length + s.origins.length + s.paths.length === 0 ? (
            <p className="ss-meta">{t('admin.csp.none')}</p>
          ) : (
            <>
              {group(t('admin.csp.directives'), s.directives)}
              {group(t('admin.csp.origins'), s.origins)}
              {group(t('admin.csp.paths'), s.paths)}
            </>
          )
        }
      </Async>
    </section>
  );
}
