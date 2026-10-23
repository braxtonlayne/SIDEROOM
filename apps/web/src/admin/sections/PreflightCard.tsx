import { Badge, Button } from '@sideroom/ui';
import { useId, useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { ActionStatus, fmtMs, useAction } from '../ui';

const BADGE = { pass: 'success', warn: 'warning', fail: 'danger' } as const;

/**
 * Admin > System "Preflight" card (RDY-OPS-005), also the last step of the Setup checklist (`compact`). One
 * button runs the server-side checks and the table shows pass, warning or fail with a sentence per row.
 */
export function PreflightCard({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const id = useId();
  const action = useAction();
  const [report, setReport] = useState<api.PreflightReport | null>(null);
  const label = (s: api.PreflightCheck['status']) =>
    s === 'pass'
      ? t('admin.preflight.statusPass')
      : s === 'warn'
        ? t('admin.preflight.statusWarn')
        : t('admin.preflight.statusFail');
  return (
    <section className="ss-card" aria-labelledby={id}>
      <h2 id={id}>{compact ? t('admin.preflight.setupTitle') : t('admin.preflight.title')}</h2>
      <p className="ss-sub">{t('admin.preflight.description')}</p>
      <div className="ss-toolbar">
        <Button
          variant="secondary"
          disabled={action.busy}
          onClick={() => {
            void action.run(() => api.runPreflight()).then((r) => r && setReport(r));
          }}
        >
          {action.busy ? t('admin.preflight.running') : t('admin.preflight.run')}
        </Button>
      </div>
      <ActionStatus error={action.error} />
      {!report && !action.busy && !action.error && (
        <p className="ss-meta">{t('admin.preflight.never')}</p>
      )}
      {report && (
        <>
          <p className="ss-sub" role="status">
            {t('admin.preflight.summary', {
              passed: report.passed,
              warned: report.warned,
              failed: report.failed,
            })}{' '}
            {report.ok ? t('admin.preflight.ready') : t('admin.preflight.notReady')}{' '}
            {t('admin.preflight.ranAt', { at: fmtMs(report.ran_at_ms) })}
          </p>
          <ul className="ss-list" aria-label={t('admin.preflight.tableLabel')}>
            {report.checks.map((c) => (
              <li key={c.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">{c.title}</div>
                  <div className="ss-meta">{c.detail}</div>
                </div>
                <Badge variant={BADGE[c.status]}>{label(c.status)}</Badge>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
