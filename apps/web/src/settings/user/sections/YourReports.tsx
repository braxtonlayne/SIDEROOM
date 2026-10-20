// "Your reports" (N-24; MOD-RPT-013; spec 3.3): what the reporter may read about their own reports, which is a
// state and a short outcome line and nothing about the case or the other party.
import { useEffect, useState } from 'react';
import { formatDate, useT, type MessageKey } from '../../../i18n';
import {
  myReports,
  type MyReport,
  type ReportCategory,
  type ReportState,
} from '../../../moderation/api';
import { CATEGORY_KEYS } from '../../../moderation/ReportDialog';
import { useReportsEnabled } from '../../../moderation/reports';
import { Card } from '../parts';

const STATE_KEYS: Record<ReportState, MessageKey> = {
  received: 'settings.privacy.reports.stateReceived',
  under_review: 'settings.privacy.reports.stateUnderReview',
  closed: 'settings.privacy.reports.stateClosed',
};

const KIND_KEYS: Record<string, MessageKey> = {
  message: 'admin.moderation.target.message',
  user: 'admin.moderation.target.user',
  channel: 'admin.moderation.target.channel',
  guild: 'admin.moderation.target.guild',
  library: 'admin.moderation.target.library',
  cinema: 'admin.moderation.target.cinema',
};

export function YourReportsCard() {
  const t = useT();
  const enabled = useReportsEnabled();
  const [items, setItems] = useState<MyReport[] | null>(null);
  useEffect(() => {
    let live = true;
    myReports()
      .then((r) => live && setItems(r))
      .catch(() => live && setItems([]));
    return () => {
      live = false;
    };
  }, []);
  // Shown while reporting is on, or when the person still has reports to read.
  if (!items || (!enabled && items.length === 0)) return null;
  return (
    <Card
      id="your-reports"
      title={t('settings.privacy.reports.title')}
      description={t('settings.privacy.reports.description')}
    >
      {items.length === 0 ? (
        <p className="us-muted">{t('settings.privacy.reports.empty')}</p>
      ) : (
        <ul className="us-list" aria-label={t('settings.privacy.reports.listLabel')}>
          {items.map((r) => (
            <li key={r.id}>
              <div className="us-row-main">
                <strong>
                  {KIND_KEYS[r.target_kind] ? t(KIND_KEYS[r.target_kind]!) : r.target_kind} ·{' '}
                  {t(CATEGORY_KEYS[r.category as ReportCategory])}
                </strong>
                <span className="us-muted">
                  {' '}
                  {t('settings.privacy.reports.filed', {
                    when: formatDate(r.created_at_ms),
                  })} · {t(STATE_KEYS[r.state])}
                  {r.outcome_line ? ` · ${r.outcome_line}` : ''}
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
