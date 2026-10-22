// Admin > Moderation > Dashboard (MOD-DASH-002, -005 to -010; docs/specs/moderation-compliance.md sections 7 and 9).
// One read of GET /moderation/dashboard: the API sends a section only to a viewer who holds the bit that reads it
// (MANAGE_CASES: inbox, health, retention, age; PLATFORM_BAN: bans; MANAGE_TAKEDOWNS: register, data requests), so
// the page simply renders what arrives. Totals only: no reporter and no staff member appears anywhere.
import { Badge, Button, Checkbox, EmptyState, Notice, ProgressBar, Select } from '@sideroom/ui';
import { useState, type ReactNode } from 'react';
import { formatNumber, useT, type MessageKey } from '../../i18n';
import * as api from '../../moderation/api';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import { ApiProblem, Async, ReasonDialog, Stat, fmtMs, useResource } from '../ui';
import { SEVERITY_KEYS, TARGET_KEYS } from './labels';

const WINDOWS = [7, 30, 90] as const;

const RETENTION_KEYS: Record<string, MessageKey> = {
  messages_held: 'admin.moderation.dash.retention.messagesHeld',
  messages_held_attachment: 'admin.moderation.dash.retention.messagesHeldAttachment',
  attachments_removed_held: 'admin.moderation.dash.retention.attachmentsRemovedHeld',
  attachments_expiry_held: 'admin.moderation.dash.retention.attachmentsExpiryHeld',
  submissions_held: 'admin.moderation.dash.retention.submissionsHeld',
};

const AGE_BUCKET_KEYS: Record<string, MessageKey> = {
  under_7d: 'admin.bans.ageUnder7',
  '7_to_30d': 'admin.bans.age7to30',
  '30_to_90d': 'admin.bans.age30to90',
  over_90d: 'admin.bans.ageOver90',
};

const n = (v: number) => formatNumber(v);

/** 5400 -> "2 hr" style: the largest whole unit that fits, in the viewer's locale. */
function duration(seconds: number): string {
  const units: [number, 'day' | 'hour' | 'minute' | 'second'][] = [
    [86_400, 'day'],
    [3_600, 'hour'],
    [60, 'minute'],
  ];
  for (const [size, unit] of units) {
    if (seconds >= size) {
      return formatNumber(Math.round(seconds / size), {
        style: 'unit',
        unit,
        unitDisplay: 'short',
      });
    }
  }
  return formatNumber(Math.max(0, Math.round(seconds)), {
    style: 'unit',
    unit: 'second',
    unitDisplay: 'short',
  });
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="adm-dash-section" aria-label={title}>
      <h3 className="adm-dash-heading">{title}</h3>
      {description && <p className="ss-sub">{description}</p>}
      {children}
    </section>
  );
}

/** A labelled list of counts drawn as bars; the number is always printed beside the bar. */
function Distribution({
  label,
  rows,
}: {
  label: string;
  rows: { id: string; label: string; count: number }[];
}) {
  const max = Math.max(1, ...rows.map((r) => r.count));
  return (
    <ul className="adm-dash-bars" aria-label={label}>
      {rows.map((r) => (
        <li key={r.id} className="adm-dash-bar">
          <span className="adm-dash-bar-label">{r.label}</span>
          <ProgressBar label={r.label} value={r.count} max={max} size={8} />
          <span className="adm-dash-bar-count">{n(r.count)}</span>
        </li>
      ))}
    </ul>
  );
}

export function DashboardTab({
  onOpenCase,
  onOpenRegister,
}: {
  onOpenCase: (id: string) => void;
  onOpenRegister?: (() => void) | undefined;
}) {
  const t = useT();
  const [days, setDays] = useState<number>(30);
  const res = useResource(`mod-dash:${days}`, () => api.getDashboard(days));
  return (
    <>
      <div className="ss-toolbar" role="group" aria-label={t('admin.moderation.dash.toolbar')}>
        <Select
          aria-label={t('admin.moderation.dash.window')}
          options={WINDOWS.map((w) => ({
            value: String(w),
            label: t('admin.moderation.dash.windowDays', { n: w }),
          }))}
          value={String(days)}
          onValueChange={(v) => setDays(Number(v))}
        />
        <Button size="sm" variant="secondary" onClick={() => void res.reload()}>
          {t('admin.moderation.dash.refresh')}
        </Button>
      </div>
      <Async res={res}>
        {(d) => (
          <>
            {d.inbox && (
              <InboxSection
                summary={d.inbox}
                onOpenCase={onOpenCase}
                onChanged={() => void res.reload()}
              />
            )}
            {d.health && <HealthSection h={d.health} />}
            {d.takedowns && <TakedownsSection v={d.takedowns} onOpen={onOpenRegister} />}
            {d.data_requests && <DataRequestsSection v={d.data_requests} />}
            {d.bans && <BansSection v={d.bans} days={d.window_days} />}
            {d.retention && <RetentionSection v={d.retention} />}
            {d.age && <AgeSection v={d.age} />}
            <p className="ss-sub">
              {t('admin.moderation.dash.generated', { when: fmtMs(d.generated_at_ms) })}
            </p>
          </>
        )}
      </Async>
    </>
  );
}

// ---- reports inbox (MOD-DASH-002) ----

function InboxSection({
  summary,
  onOpenCase,
  onChanged,
}: {
  summary: api.DashInbox;
  onOpenCase: (id: string) => void;
  onChanged: () => void;
}) {
  const t = useT();
  const page = useResource(`mod-inbox:${summary.groups}:${summary.reports}`, () =>
    api.listInbox({ limit: 50 }),
  );
  const [picked, setPicked] = useState<string[]>([]);
  const [dismissing, setDismissing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const items = page.data?.items ?? [];
  const toggle = (id: string, on: boolean) =>
    setPicked((p) => (on ? [...p.filter((x) => x !== id), id] : p.filter((x) => x !== id)));
  return (
    <Section
      title={t('admin.moderation.dash.inbox.title')}
      description={t('admin.moderation.dash.inbox.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.inbox.groups')} value={n(summary.groups)} />
        <Stat label={t('admin.moderation.dash.inbox.reports')} value={n(summary.reports)} />
        <Stat
          label={t('admin.moderation.dash.inbox.oldest')}
          value={summary.oldest_seconds == null ? '-' : duration(summary.oldest_seconds)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ApiProblem error={page.error} />
      {page.data && items.length === 0 && (
        <EmptyState
          title={t('admin.moderation.dash.inbox.emptyTitle')}
          description={t('admin.moderation.dash.inbox.emptyDescription')}
        />
      )}
      {items.length > 0 && (
        <>
          <div className="ss-toolbar">
            <Checkbox
              label={t('admin.moderation.dash.inbox.selectAll')}
              checked={picked.length === items.length}
              onChange={(e) => setPicked(e.target.checked ? items.map((g) => g.case_id) : [])}
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={picked.length === 0}
              onClick={() => setDismissing(true)}
            >
              {t('admin.moderation.dash.inbox.dismissSelected', { n: picked.length })}
            </Button>
          </div>
          <ul className="ss-list" aria-label={t('admin.moderation.dash.inbox.listLabel')}>
            {items.map((g) => (
              <li key={g.case_id} className="ss-row">
                <Checkbox
                  label={t('admin.moderation.dash.inbox.pick', { id: g.case_id })}
                  checked={picked.includes(g.case_id)}
                  onChange={(e) => toggle(g.case_id, e.target.checked)}
                />
                <div className="ss-grow">
                  <div className="ss-name">
                    {g.summary || t(TARGET_KEYS[g.target_kind])}{' '}
                    {g.categories.map((c) => (
                      <Badge key={c}>{t(CATEGORY_KEYS[c])}</Badge>
                    ))}{' '}
                    {g.severity === 'serious' && (
                      <Badge variant="danger">{t(SEVERITY_KEYS[g.severity])}</Badge>
                    )}{' '}
                    {g.reports > 1 && (
                      <Badge variant="warning">
                        {t('admin.moderation.dash.inbox.duplicates', { n: g.reports })}
                      </Badge>
                    )}
                  </div>
                  <div className="ss-meta">
                    {t('admin.moderation.dash.inbox.reportCount', { n: g.reports })} ·{' '}
                    {t('admin.moderation.dash.inbox.first', { when: fmtMs(g.first_report_at_ms) })}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t('admin.moderation.dash.inbox.openAria', { id: g.case_id })}
                  onClick={() => onOpenCase(g.case_id)}
                >
                  {t('admin.moderation.dash.inbox.open')}
                </Button>
              </li>
            ))}
          </ul>
        </>
      )}
      <ReasonDialog
        open={dismissing}
        title={t('admin.moderation.dash.inbox.dismissTitle', { n: picked.length })}
        description={t('admin.moderation.dash.inbox.dismissDescription')}
        confirmLabel={t('admin.moderation.dash.inbox.dismissConfirm')}
        onCancel={() => setDismissing(false)}
        onConfirm={async (reason) => {
          const out = await api.dismissInbox(picked, reason);
          setNotice(
            t('admin.moderation.dash.inbox.dismissed', {
              done: out.dismissed.length,
              skipped: out.skipped.length,
            }),
          );
          setPicked([]);
          setDismissing(false);
          onChanged();
        }}
      />
    </Section>
  );
}

// ---- health (MOD-DASH-008) ----

function HealthSection({ h }: { h: api.DashHealth }) {
  const t = useT();
  const outcome = (key: string) => h.appeal_outcomes.find((o) => o.key === key)?.count ?? 0;
  return (
    <Section
      title={t('admin.moderation.dash.health.title')}
      description={t('admin.moderation.dash.health.description', { n: h.window_days })}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.health.reports')} value={n(h.reports)} />
        <Stat label={t('admin.moderation.dash.health.casesOpened')} value={n(h.cases_opened)} />
        <Stat
          label={t('admin.moderation.dash.health.median')}
          value={
            h.median_first_action_seconds == null ? '-' : duration(h.median_first_action_seconds)
          }
          hint={t('admin.moderation.dash.health.waiting', { n: h.cases_waiting })}
        />
        <Stat
          label={t('admin.moderation.dash.health.reopenRate')}
          value={
            h.reopen_rate == null
              ? '-'
              : formatNumber(h.reopen_rate, { style: 'percent', maximumFractionDigits: 0 })
          }
          hint={t('admin.moderation.dash.health.reopenHint', {
            reopened: h.reopened,
            closed: h.closed,
          })}
        />
        <Stat
          label={t('admin.moderation.dash.health.appeals')}
          value={t('admin.moderation.dash.health.appealsValue', {
            upheld: outcome('upheld'),
            overturned: outcome('overturned'),
          })}
          hint={t('admin.moderation.dash.health.appealsOpen', { n: h.appeals_open })}
        />
      </div>
      <Distribution
        label={t('admin.moderation.dash.health.perDay')}
        rows={h.reports_per_day.map((d) => ({ id: d.day, label: d.day, count: d.reports }))}
      />
    </Section>
  );
}

// ---- takedown register (MOD-DASH-005) and data requests (MOD-DASH-006) ----

function TakedownsSection({
  v,
  onOpen,
}: {
  v: api.DashTakedowns;
  onOpen?: (() => void) | undefined;
}) {
  const t = useT();
  return (
    <Section
      title={t('admin.moderation.dash.takedowns.title')}
      description={t('admin.moderation.dash.takedowns.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.takedowns.undecided')} value={n(v.undecided)} />
        <Stat
          label={t('admin.moderation.dash.takedowns.overdue')}
          value={n(v.triage_overdue)}
          hint={v.triage_overdue > 0 ? t('admin.moderation.dash.takedowns.overdueHint') : undefined}
        />
        <Stat
          label={t('admin.moderation.dash.takedowns.counterNoticed')}
          value={n(v.counter_noticed)}
          hint={t('admin.moderation.dash.takedowns.restoreDue', { n: v.restore_due })}
        />
        <Stat
          label={t('admin.moderation.dash.takedowns.received', { n: v.window_days })}
          value={n(v.received_in_window)}
          hint={t('admin.moderation.dash.takedowns.removed', { n: v.removed_in_window })}
        />
      </div>
      {onOpen && (
        <Button size="sm" variant="secondary" onClick={onOpen}>
          {t('admin.moderation.dash.takedowns.open')}
        </Button>
      )}
    </Section>
  );
}

function DataRequestsSection({ v }: { v: api.DashDataRequests }) {
  const t = useT();
  return (
    <Section
      title={t('admin.moderation.dash.requests.title')}
      description={t('admin.moderation.dash.requests.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.requests.exports')} value={n(v.open_exports)} />
        <Stat label={t('admin.moderation.dash.requests.erasures')} value={n(v.open_erasures)} />
        <Stat label={t('admin.moderation.dash.requests.overdue')} value={n(v.overdue)} />
        <Stat label={t('admin.moderation.dash.requests.dueSoon')} value={n(v.due_soon)} />
        <Stat
          label={t('admin.moderation.dash.requests.completed', { n: v.window_days })}
          value={n(v.completed_in_window)}
        />
      </div>
      <a href="/admin/data-requests">{t('admin.moderation.dash.requests.open')}</a>
    </Section>
  );
}

// ---- bans (MOD-DASH-007) ----

function BansSection({ v, days }: { v: api.DashBans; days: number }) {
  const t = useT();
  const reasonLabel = (k: string) =>
    k === 'none'
      ? t('admin.bans.reasonNone')
      : CATEGORY_KEYS[k as api.ReportCategory]
        ? t(CATEGORY_KEYS[k as api.ReportCategory])
        : k;
  return (
    <Section
      title={t('admin.moderation.dash.bans.title')}
      description={t('admin.moderation.dash.bans.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.bans.active')} value={n(v.active)} />
        <Stat
          label={t('admin.moderation.dash.bans.lifted', { n: days })}
          value={n(v.lifted_in_window)}
        />
      </div>
      <Distribution
        label={t('admin.moderation.dash.bans.byReason')}
        rows={v.by_reason.map((r) => ({ id: r.key, label: reasonLabel(r.key), count: r.count }))}
      />
      <Distribution
        label={t('admin.moderation.dash.bans.bySeverity')}
        rows={v.by_severity.map((r) => ({
          id: r.key,
          label: r.key === 'serious' ? t(SEVERITY_KEYS.serious) : t(SEVERITY_KEYS.standard),
          count: r.count,
        }))}
      />
      <Distribution
        label={t('admin.moderation.dash.bans.byAge')}
        rows={v.by_age.map((r) => ({
          id: r.key,
          label: AGE_BUCKET_KEYS[r.key] ? t(AGE_BUCKET_KEYS[r.key]!) : r.key,
          count: r.count,
        }))}
      />
      <a href="/admin/bans">{t('admin.moderation.dash.bans.open')}</a>
    </Section>
  );
}

// ---- retention check (MOD-DASH-009) ----

function RetentionSection({ v }: { v: api.DashRetention }) {
  const t = useT();
  return (
    <Section
      title={t('admin.moderation.dash.retention.title')}
      description={t('admin.moderation.dash.retention.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.retention.holdsActive')} value={n(v.holds_active)} />
        <Stat
          label={t('admin.moderation.dash.retention.holdsReview')}
          value={n(v.holds_review_due)}
        />
      </div>
      <ul className="ss-list" aria-label={t('admin.moderation.dash.retention.listLabel')}>
        {v.rows.map((r) => (
          <li key={r.key} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">
                {RETENTION_KEYS[r.key] ? t(RETENTION_KEYS[r.key]!) : r.key}
              </div>
            </div>
            <Badge variant={r.count > 0 ? 'warning' : 'neutral'}>
              {r.capped ? t('admin.moderation.dash.retention.atLeast', { n: r.count }) : n(r.count)}
            </Badge>
          </li>
        ))}
      </ul>
    </Section>
  );
}

// ---- age and child safety (MOD-DASH-010) ----

function AgeSection({ v }: { v: api.DashAge }) {
  const t = useT();
  return (
    <Section
      title={t('admin.moderation.dash.age.title')}
      description={t('admin.moderation.dash.age.description')}
    >
      <div className="adm-stats">
        <Stat label={t('admin.moderation.dash.age.channels')} value={n(v.restricted_channels)} />
        <Stat label={t('admin.moderation.dash.age.series')} value={n(v.restricted_series)} />
        <Stat label={t('admin.moderation.dash.age.titles')} value={n(v.restricted_titles)} />
        <Stat
          label={t('admin.moderation.dash.age.resets', { n: v.window_days })}
          value={n(v.age_gate_resets)}
        />
        <Stat
          label={t('admin.moderation.dash.age.openCases')}
          value={n(v.child_safety_open_cases)}
        />
        <Stat
          label={t('admin.moderation.dash.age.reports', { n: v.window_days })}
          value={n(v.child_safety_reports)}
        />
        <Stat
          label={t('admin.moderation.dash.age.activeBans')}
          value={n(v.child_safety_active_bans)}
        />
      </div>
    </Section>
  );
}
