import { Badge, Button, EmptyState, Notice, Select, Switch } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  REPORT_CATEGORIES,
  getDashboard,
  type DashBans,
  type ReportCategory,
} from '../../moderation/api';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import { PersonDrawer } from './PersonDrawer';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged, useResource } from '../ui';

/** The age filter (MOD-BAN-003): the same buckets the dashboard counts (MOD-DASH-007). */
type AgeFilter = 'any' | 'under7' | 'd7to30' | 'd30to90' | 'over90';
const AGE_KEYS: Record<AgeFilter, MessageKey> = {
  any: 'admin.bans.ageAny',
  under7: 'admin.bans.ageUnder7',
  d7to30: 'admin.bans.age7to30',
  d30to90: 'admin.bans.age30to90',
  over90: 'admin.bans.ageOver90',
};
const AGE_RANGE: Record<AgeFilter, Pick<api.BanQuery, 'older_than_days' | 'newer_than_days'>> = {
  any: {},
  under7: { newer_than_days: 7 },
  d7to30: { older_than_days: 7, newer_than_days: 30 },
  d30to90: { older_than_days: 30, newer_than_days: 90 },
  over90: { older_than_days: 90 },
};

/** The dashboard's age buckets as the list's age filter. */
const BUCKET_FILTER: Record<string, AgeFilter> = {
  under_7d: 'under7',
  '7_to_30d': 'd7to30',
  '30_to_90d': 'd30to90',
  over_90d: 'over90',
};

/**
 * The ban figures above the list (MOD-DASH-007, D-340): one dashboard read, which answers only the `bans` section to
 * a PLATFORM_BAN holder. A refusal or a missing section draws nothing. A count is a button that sets the list filter.
 */
function BanFigures({
  onSeverity,
  onReason,
  onAge,
}: {
  onSeverity: (v: api.BanCategory) => void;
  onReason: (v: ReportCategory | 'none') => void;
  onAge: (v: AgeFilter) => void;
}) {
  const t = useT();
  const [days, setDays] = useState(30);
  const res = useResource<DashBans | null>(`ban-figures:${days}`, async () => {
    try {
      return (await getDashboard(days)).bans ?? null;
    } catch {
      return null;
    }
  });
  const b = res.data;
  if (!b) return null;
  const reasonLabel = (k: string) =>
    k === 'none'
      ? t('admin.bans.reasonNone')
      : CATEGORY_KEYS[k as ReportCategory]
        ? t(CATEGORY_KEYS[k as ReportCategory])
        : k;
  const chip = (key: string, label: string, count: number, onClick: () => void) => (
    <Button
      key={key}
      size="sm"
      variant="secondary"
      aria-label={t('admin.bans.figures.chipAria', { label, count })}
      onClick={onClick}
    >
      {label}: {count}
    </Button>
  );
  return (
    <section className="adm-dash-section" aria-label={t('admin.bans.figures.label')}>
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.moderation.dash.window')}
          options={[7, 30, 90].map((w) => ({
            value: String(w),
            label: t('admin.moderation.dash.windowDays', { n: w }),
          }))}
          value={String(days)}
          onValueChange={(v) => setDays(Number(v))}
        />
        <span className="ss-sub">
          {t('admin.bans.figures.summary', { active: b.active, lifted: b.lifted_in_window })}
        </span>
      </div>
      <div className="adm-actions" role="group" aria-label={t('admin.bans.figures.byReason')}>
        {b.by_reason.map((r) =>
          chip(r.key, reasonLabel(r.key), r.count, () =>
            onReason(r.key as ReportCategory | 'none'),
          ),
        )}
      </div>
      <div className="adm-actions" role="group" aria-label={t('admin.bans.figures.bySeverity')}>
        {b.by_severity.map((r) =>
          chip(
            r.key,
            r.key === 'serious'
              ? t('admin.bans.severitySerious')
              : t('admin.bans.severityStandard'),
            r.count,
            () => onSeverity(r.key as api.BanCategory),
          ),
        )}
      </div>
      <div className="adm-actions" role="group" aria-label={t('admin.bans.figures.byAge')}>
        {b.by_age.map((r) => {
          const f = BUCKET_FILTER[r.key];
          return f ? chip(r.key, t(AGE_KEYS[f]), r.count, () => onAge(f)) : null;
        })}
      </div>
    </section>
  );
}

export function BansSection() {
  const t = useT();
  const [activeOnly, setActiveOnly] = useState(true);
  const [severity, setSeverity] = useState<api.BanCategory | 'any'>('any');
  const [reason, setReason] = useState<ReportCategory | 'none' | 'any'>('any');
  const [age, setAge] = useState<AgeFilter>('any');
  const [target, setTarget] = useState<api.BanRecord | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [person, setPerson] = useState<string | null>(null);
  const paged = usePaged<api.BanRecord>(
    `bans:${activeOnly}:${severity}:${reason}:${age}`,
    async (after) => {
      const p = await api.listBans({
        active_only: activeOnly,
        ...(severity !== 'any' ? { category: severity } : {}),
        ...(reason !== 'any' ? { reason_category: reason } : {}),
        ...AGE_RANGE[age],
        after,
        limit: 50,
      });
      return { items: p.items, next: p.next_after };
    },
  );

  return (
    <>
      <SectionHeader title={t('admin.bans.title')} description={t('admin.bans.description')} />
      <BanFigures
        onSeverity={(v) => {
          setActiveOnly(true);
          setSeverity(v);
        }}
        onReason={(v) => {
          setActiveOnly(true);
          setReason(v);
        }}
        onAge={(v) => {
          setActiveOnly(true);
          setAge(v);
        }}
      />
      <div className="ss-toolbar" role="group" aria-label={t('admin.bans.filters')}>
        <Switch
          label={t('admin.bans.onlyActive')}
          checked={activeOnly}
          onChange={(e) => setActiveOnly(e.target.checked)}
        />
        <Select
          aria-label={t('admin.bans.severityFilter')}
          options={[
            { value: 'any', label: t('admin.bans.severityAny') },
            { value: 'serious', label: t('admin.bans.severitySerious') },
            { value: 'standard', label: t('admin.bans.severityStandard') },
          ]}
          value={severity}
          onValueChange={(v) => setSeverity(v as api.BanCategory | 'any')}
        />
        <Select
          aria-label={t('admin.bans.reasonFilter')}
          options={[
            { value: 'any', label: t('admin.bans.reasonAny') },
            ...REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) })),
            { value: 'none', label: t('admin.bans.reasonNone') },
          ]}
          value={reason}
          onValueChange={(v) => setReason(v as ReportCategory | 'none' | 'any')}
        />
        <Select
          aria-label={t('admin.bans.ageFilter')}
          options={(Object.keys(AGE_KEYS) as AgeFilter[]).map((a) => ({
            value: a,
            label: t(AGE_KEYS[a]),
          }))}
          value={age}
          onValueChange={(v) => setAge(v as AgeFilter)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.bans.emptyTitle')}
          description={t('admin.bans.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.bans.listLabel')}>
          {paged.items.map((b) => (
            <li key={b.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t('admin.bans.person')}{' '}
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={t('admin.bans.openPersonAria', { id: b.person_id })}
                    onClick={() => setPerson(b.person_id)}
                  >
                    <span className="adm-mono">{b.person_id}</span>
                  </Button>{' '}
                  <Badge variant={b.category === 'serious' ? 'danger' : 'warning'}>
                    {b.category}
                  </Badge>{' '}
                  {b.reason_category && (
                    <>
                      <Badge>{reasonLabel(t, b.reason_category)}</Badge>{' '}
                    </>
                  )}
                  {b.lifted_at_ms ? (
                    <Badge>{t('admin.bans.lifted')}</Badge>
                  ) : (
                    <Badge variant="danger">{t('admin.bans.active')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {b.note || t('admin.bans.noNote')} ·{' '}
                  {t('admin.bans.banned', { when: fmtMs(b.created_at_ms) })}
                  {b.lifted_at_ms
                    ? ` · ${t('admin.bans.liftedAt', { when: fmtMs(b.lifted_at_ms) })}`
                    : ''}
                </div>
              </div>
              {!b.lifted_at_ms && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.bans.liftAria', { id: b.person_id })}
                  onClick={() => setTarget(b)}
                >
                  {t('admin.bans.liftButton')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />
      {person && <PersonDrawer personId={person} onClose={() => setPerson(null)} />}
      <ReasonDialog
        open={!!target}
        title={t('admin.bans.liftTitle')}
        description={t('admin.bans.liftDescription')}
        confirmLabel={t('admin.bans.liftButton')}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          const r = await api.unbanPerson(target.person_id, reason);
          setNotice(
            t('admin.bans.liftedNotice', {
              reactivated: r.accounts_reactivated,
              left: r.accounts_left_deactivated,
            }),
          );
          setTarget(null);
          paged.reload();
        }}
      />
    </>
  );
}

/** The label of a stored `reason_category`; an unknown value (a later forward-only addition) shows as is. */
function reasonLabel(t: ReturnType<typeof useT>, value: string): string {
  const key = CATEGORY_KEYS[value as ReportCategory];
  return key ? t(key) : value;
}
