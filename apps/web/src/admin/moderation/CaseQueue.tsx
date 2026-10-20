// The case queue (MOD-CASE-011; spec 9.1): open cases by age, with filters; claim from the case page.
import { Badge, Button, EmptyState, Select, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import * as api from '../../moderation/api';
import { ApiProblem, LoadMore, fmtMs, usePaged, useAdminViewer } from '../ui';
import { ROUTING_KEYS, SEVERITY_KEYS, STATE_KEYS, TARGET_KEYS } from './labels';
import { TakedownBadge } from './TakedownBadge';

export function CaseQueue({
  onOpen,
  onOpenNotice,
}: {
  onOpen: (id: string) => void;
  onOpenNotice?: ((noticeId: string) => void) | undefined;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const [view, setView] = useState<'work' | 'all'>('work');
  const [category, setCategory] = useState<api.ReportCategory | 'any'>('any');
  const [severity, setSeverity] = useState<api.Severity | 'any'>('any');
  const [mine, setMine] = useState(false);
  const [oldestFirst, setOldestFirst] = useState(true);
  const [source, setSource] = useState<'any' | 'takedown'>('any');
  const key = `cases:${view}:${category}:${severity}:${mine}:${oldestFirst}:${source}`;
  const paged = usePaged<api.CaseSummary>(key, async (cursor) => {
    const page = await api.listCases({
      open_only: view === 'work',
      category: category === 'any' ? '' : category,
      severity: severity === 'any' ? '' : severity,
      ...(mine ? { assignee: viewer.userId } : {}),
      oldest_first: oldestFirst,
      source: source === 'takedown' ? 'takedown' : '',
      cursor,
      limit: 25,
    });
    return { items: page.items, next: page.next_cursor };
  });

  return (
    <>
      <div className="ss-toolbar" role="group" aria-label={t('admin.moderation.queue.filters')}>
        <Select
          aria-label={t('admin.moderation.queue.viewLabel')}
          options={[
            { value: 'work', label: t('admin.moderation.queue.viewWork') },
            { value: 'all', label: t('admin.moderation.queue.viewAll') },
          ]}
          value={view}
          onValueChange={(v) => setView(v as 'work' | 'all')}
        />
        <Select
          aria-label={t('admin.moderation.queue.categoryLabel')}
          options={[
            { value: 'any', label: t('admin.moderation.queue.categoryAny') },
            ...api.REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) })),
          ]}
          value={category}
          onValueChange={(v) => setCategory(v as api.ReportCategory | 'any')}
        />
        <Select
          aria-label={t('admin.moderation.queue.severityLabel')}
          options={[
            { value: 'any', label: t('admin.moderation.queue.severityAny') },
            { value: 'serious', label: t(SEVERITY_KEYS.serious) },
            { value: 'standard', label: t(SEVERITY_KEYS.standard) },
          ]}
          value={severity}
          onValueChange={(v) => setSeverity(v as api.Severity | 'any')}
        />
        <Select
          aria-label={t('admin.moderation.queue.sourceLabel')}
          options={[
            { value: 'any', label: t('admin.moderation.queue.sourceAny') },
            { value: 'takedown', label: t('admin.moderation.queue.sourceTakedown') },
          ]}
          value={source}
          onValueChange={(v) => setSource(v as 'any' | 'takedown')}
        />
        <Switch
          label={t('admin.moderation.queue.mine')}
          checked={mine}
          onChange={(e) => setMine(e.target.checked)}
        />
        <Switch
          label={t('admin.moderation.queue.oldestFirst')}
          checked={oldestFirst}
          onChange={(e) => setOldestFirst(e.target.checked)}
        />
      </div>
      <ApiProblem error={paged.error} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.moderation.queue.emptyTitle')}
          description={t('admin.moderation.queue.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.moderation.queue.listLabel')}>
          {paged.items.map((c) => (
            <li key={c.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  <Badge variant={c.severity === 'serious' ? 'danger' : 'neutral'}>
                    {t(SEVERITY_KEYS[c.severity])}
                  </Badge>{' '}
                  {t(CATEGORY_KEYS[c.category])} · {t(TARGET_KEYS[c.target_kind])}{' '}
                  <Badge variant={c.state === 'open' ? 'accent' : 'neutral'}>
                    {t(STATE_KEYS[c.state])}
                  </Badge>
                  {c.takedown && (
                    <>
                      {' '}
                      <TakedownBadge takedown={c.takedown} onOpen={onOpenNotice} />
                    </>
                  )}
                  {c.escalated && (
                    <>
                      {' '}
                      <Badge variant="warning">{t('admin.moderation.queue.escalated')}</Badge>
                    </>
                  )}
                </div>
                <div className="ss-meta">
                  {c.summary ? `${c.summary} · ` : ''}
                  {t('admin.moderation.queue.reports', { n: c.report_count })}
                  {c.active_holds > 0
                    ? ` · ${t('admin.moderation.queue.holds', { n: c.active_holds })}`
                    : ''}{' '}
                  · {t(ROUTING_KEYS[c.routing])} ·{' '}
                  {t('admin.moderation.queue.openedAt', { when: fmtMs(c.opened_at_ms) })} ·{' '}
                  {c.assignee_id
                    ? t('admin.moderation.queue.assignedTo', { id: c.assignee_id })
                    : t('admin.moderation.queue.unassigned')}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.moderation.queue.openAria', { id: c.id })}
                onClick={() => onOpen(c.id)}
              >
                {t('admin.moderation.queue.open')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />
    </>
  );
}
