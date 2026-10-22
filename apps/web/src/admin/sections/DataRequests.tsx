// Data-subject request queue (M-25 step c, MOD-DSR; D-295): export and erasure requests with the due date
// computed when they were entered, overdue marking, one extension, and the runs. Needs MANAGE_TAKEDOWNS.
import {
  Badge,
  Button,
  Checkbox,
  EmptyState,
  Notice,
  Select,
  Switch,
  TextInput,
} from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  ActionStatus,
  Async,
  FormDialog,
  ReasonDialog,
  SectionHeader,
  fmtMs,
  useAction,
  useResource,
} from '../ui';

const DAY_MS = 86_400_000;

const KIND_KEY: Record<api.DataRequestKind, MessageKey> = {
  export: 'admin.dataRequests.kind.export',
  erasure: 'admin.dataRequests.kind.erasure',
};
const STATE_KEY: Record<api.DataRequestState, MessageKey> = {
  open: 'admin.dataRequests.state.open',
  verified: 'admin.dataRequests.state.verified',
  running: 'admin.dataRequests.state.running',
  completed: 'admin.dataRequests.state.completed',
  declined: 'admin.dataRequests.state.declined',
};

const isActive = (r: api.DataRequest) =>
  r.state === 'open' || r.state === 'verified' || r.state === 'running';

/** Whole days between now and the date: positive when still ahead. */
const daysUntil = (ms: number, now: number) => Math.trunc((ms - now) / DAY_MS);

type Dialog =
  { kind: 'verify' | 'decline' | 'extend' | 'run' | 'restrict' | 'rectify'; req: api.DataRequest } | { kind: 'new' } | null;

export function DataRequestsSection() {
  const t = useT();
  const [activeOnly, setActiveOnly] = useState(true);
  const list = useResource(`admin:data-requests:${activeOnly}`, () =>
    api.listDataRequests(activeOnly),
  );
  const [dialog, setDialog] = useState<Dialog>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [draft, setDraft] = useState({
    kind: 'export' as api.DataRequestKind,
    account: '',
    received: '',
    note: '',
  });
  const [days, setDays] = useState('30');
  const [deleteMessages, setDeleteMessages] = useState(false);
  const [personId, setPersonId] = useState('');
  const action = useAction();
  const now = Date.now();

  const done = async (msg: string) => {
    setDialog(null);
    setNotice(msg);
    await list.reload();
  };

  const daysNum = Number(days);
  const daysValid = Number.isInteger(daysNum) && daysNum >= 1 && daysNum <= 60;
  const receivedMs = draft.received ? new Date(draft.received).getTime() : undefined;
  const newValid =
    /^\d+$/.test(draft.account.trim()) &&
    (receivedMs === undefined || (Number.isFinite(receivedMs) && receivedMs <= now));

  const catalog = list.data?.retained_catalog ?? [];

  return (
    <>
      <SectionHeader
        title={t('admin.dataRequests.title')}
        description={t('admin.dataRequests.description', {
          days: list.data?.deadline_days ?? 30,
        })}
        actions={
          <Button
            onClick={() => {
              setDraft({ kind: 'export', account: '', received: '', note: '' });
              setDialog({ kind: 'new' });
            }}
          >
            {t('admin.dataRequests.new')}
          </Button>
        }
      />
      <div className="ss-toolbar">
        <Switch
          label={t('admin.dataRequests.onlyOpen')}
          checked={activeOnly}
          onChange={(e) => setActiveOnly(e.target.checked)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />
      <Async res={list} label={t('admin.dataRequests.loading')}>
        {(page) =>
          page.items.length === 0 ? (
            <EmptyState
              title={t('admin.dataRequests.emptyTitle')}
              description={t('admin.dataRequests.emptyDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('admin.dataRequests.listLabel')}>
              {page.items.map((r) => {
                const left = daysUntil(r.effective_due_ms, now);
                const active = isActive(r);
                return (
                  <li key={r.id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">
                        {r.username} <Badge>{t(KIND_KEY[r.kind])}</Badge>{' '}
                        <Badge variant={r.state === 'completed' ? 'success' : 'neutral'}>
                          {t(STATE_KEY[r.state])}
                        </Badge>{' '}
                        {r.overdue && (
                          <Badge variant="danger">{t('admin.dataRequests.overdueBadge')}</Badge>
                        )}{' '}
                        {r.activity_hidden && active && (
                          <Badge variant="warning">{t('admin.dataRequests.hiddenBadge')}</Badge>
                        )}
                      </div>
                      <div className="ss-meta">
                        {t('admin.dataRequests.received', { when: fmtMs(r.received_at_ms) })}
                        {active && (
                          <>
                            {' · '}
                            <span data-overdue={r.overdue ? 'true' : undefined}>
                              {r.overdue
                                ? t('admin.dataRequests.overdue', { days: Math.max(1, -left) })
                                : left <= 0
                                  ? t('admin.dataRequests.dueToday')
                                  : t('admin.dataRequests.dueIn', { days: left })}
                            </span>
                            {' · '}
                            {t('admin.dataRequests.due', { when: fmtMs(r.effective_due_ms) })}
                          </>
                        )}
                        {r.verified_at_ms &&
                          ` · ${t('admin.dataRequests.verifiedBy', { when: fmtMs(r.verified_at_ms) })}`}
                        {r.completed_at_ms &&
                          ` · ${t('admin.dataRequests.closedAt', { when: fmtMs(r.completed_at_ms) })}`}
                        {r.note && (
                          <>
                            <br />
                            {r.note}
                          </>
                        )}
                        {r.extended_until_ms && (
                          <>
                            <br />
                            {t('admin.dataRequests.extended', {
                              when: fmtMs(r.extended_until_ms),
                              reason: r.extension_reason ?? '',
                            })}{' '}
                            ({t('admin.dataRequests.originalDue', { when: fmtMs(r.due_at_ms) })}){' '}
                            {r.extended_late && (
                              <Badge variant="warning">
                                {t('admin.dataRequests.extendedLate')}
                              </Badge>
                            )}
                            <br />
                            {r.member_notified_at_ms
                              ? t('admin.dataRequests.notified', {
                                  date: fmtMs(r.member_notified_at_ms),
                                })
                              : t('admin.dataRequests.notNotified')}
                          </>
                        )}
                        {r.activity_hidden && active && (
                          <>
                            <br />
                            {t('admin.dataRequests.hiddenNote', {
                              when: fmtMs(r.activity_hidden_at_ms ?? 0),
                              reason: r.activity_hidden_reason ?? '',
                            })}
                          </>
                        )}
                        {r.outcome && (
                          <>
                            <br />
                            {r.outcome}
                          </>
                        )}
                        {r.retained.length > 0 && (
                          <>
                            <br />
                            {t('admin.dataRequests.keptHeading')}:{' '}
                            {r.retained
                              .map((id) => catalog.find((c) => c.id === id)?.label ?? id)
                              .join(', ')}
                          </>
                        )}
                      </div>
                    </div>
                    {r.state === 'open' && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.dataRequests.verifyAria', { name: r.username })}
                        onClick={() => setDialog({ kind: 'verify', req: r })}
                      >
                        {t('admin.dataRequests.verify')}
                      </Button>
                    )}
                    {r.state === 'verified' && (
                      <Button
                        size="sm"
                        variant={r.kind === 'erasure' ? 'danger' : 'primary'}
                        aria-label={t('admin.dataRequests.runAria', { name: r.username })}
                        onClick={() => {
                          setDeleteMessages(false);
                          setDialog({ kind: 'run', req: r });
                        }}
                      >
                        {t('admin.dataRequests.run')}
                      </Button>
                    )}
                    {active && !r.extended_until_ms && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.dataRequests.extendAria', { name: r.username })}
                        onClick={() => {
                          setDays('30');
                          setDialog({ kind: 'extend', req: r });
                        }}
                      >
                        {t('admin.dataRequests.extend')}
                      </Button>
                    )}
                    {active && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={
                          r.activity_hidden
                            ? t('admin.dataRequests.showActivityAria', { name: r.username })
                            : t('admin.dataRequests.hideActivityAria', { name: r.username })
                        }
                        onClick={() => setDialog({ kind: 'restrict', req: r })}
                      >
                        {r.activity_hidden
                          ? t('admin.dataRequests.showActivity')
                          : t('admin.dataRequests.hideActivity')}
                      </Button>
                    )}
                    {active && (
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={t('admin.dataRequests.rectifyAria', { name: r.username })}
                        onClick={() => {
                          setPersonId('');
                          setDialog({ kind: 'rectify', req: r });
                        }}
                      >
                        {t('admin.dataRequests.rectify')}
                      </Button>
                    )}
                    {(r.state === 'open' || r.state === 'verified') && (
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={t('admin.dataRequests.declineAria', { name: r.username })}
                        onClick={() => setDialog({ kind: 'decline', req: r })}
                      >
                        {t('admin.dataRequests.decline')}
                      </Button>
                    )}
                  </li>
                );
              })}
            </ul>
          )
        }
      </Async>

      <FormDialog
        open={dialog?.kind === 'new'}
        title={t('admin.dataRequests.newTitle')}
        description={t('admin.dataRequests.newDescription')}
        confirmLabel={t('admin.dataRequests.newConfirm')}
        valid={newValid}
        onCancel={() => setDialog(null)}
        onConfirm={async () => {
          await api.createDataRequest({
            kind: draft.kind,
            account_id: draft.account.trim(),
            ...(receivedMs !== undefined ? { received_at_ms: receivedMs } : {}),
            ...(draft.note.trim() ? { note: draft.note.trim() } : {}),
          });
          await done(t('admin.dataRequests.created'));
        }}
      >
        <Select
          aria-label={t('admin.dataRequests.fieldKind')}
          options={(Object.keys(KIND_KEY) as api.DataRequestKind[]).map((k) => ({
            value: k,
            label: t(KIND_KEY[k]),
          }))}
          value={draft.kind}
          onValueChange={(v) => setDraft({ ...draft, kind: v as api.DataRequestKind })}
        />
        <TextInput
          label={t('admin.dataRequests.fieldAccount')}
          hint={t('admin.dataRequests.fieldAccountHint')}
          inputMode="numeric"
          value={draft.account}
          onChange={(e) => setDraft({ ...draft, account: e.target.value })}
        />
        <TextInput
          label={t('admin.dataRequests.fieldReceived')}
          hint={t('admin.dataRequests.fieldReceivedHint')}
          type="datetime-local"
          value={draft.received}
          onChange={(e) => setDraft({ ...draft, received: e.target.value })}
        />
        <TextInput
          label={t('admin.dataRequests.fieldNote')}
          hint={t('admin.dataRequests.fieldNoteHint')}
          maxLength={1000}
          value={draft.note}
          onChange={(e) => setDraft({ ...draft, note: e.target.value })}
        />
      </FormDialog>

      <ReasonDialog
        open={dialog?.kind === 'verify'}
        title={t('admin.dataRequests.verifyTitle')}
        description={t('admin.dataRequests.verifyDescription')}
        confirmLabel={t('admin.dataRequests.verify')}
        reasonLabel={t('admin.dataRequests.verifyReason')}
        onCancel={() => setDialog(null)}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'verify') return;
          await api.verifyDataRequest(dialog.req.id, reason);
          await done(t('admin.dataRequests.ran'));
        }}
      />

      <ReasonDialog
        open={dialog?.kind === 'decline'}
        title={t('admin.dataRequests.declineTitle')}
        description={t('admin.dataRequests.declineDescription')}
        confirmLabel={t('admin.dataRequests.decline')}
        reasonLabel={t('admin.dataRequests.declineReason')}
        danger
        onCancel={() => setDialog(null)}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'decline') return;
          await api.declineDataRequest(dialog.req.id, reason);
          await done(t('admin.dataRequests.ran'));
        }}
      />

      <ReasonDialog
        open={dialog?.kind === 'extend'}
        title={t('admin.dataRequests.extendTitle')}
        description={t('admin.dataRequests.extendDescription')}
        confirmLabel={t('admin.dataRequests.extendConfirm')}
        reasonLabel={t('admin.dataRequests.extendReason')}
        valid={daysValid}
        onCancel={() => setDialog(null)}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'extend') return;
          await api.extendDataRequest(dialog.req.id, daysNum, reason);
          await done(t('admin.dataRequests.extendedNotice'));
        }}
      >
        <TextInput
          label={t('admin.dataRequests.extendDays')}
          hint={t('admin.dataRequests.extend.daysHint')}
          type="number"
          min={1}
          max={60}
          value={days}
          onChange={(e) => setDays(e.target.value)}
        />
        <p className="ss-sub">{t('admin.dataRequests.extend.reasonHint')}</p>
      </ReasonDialog>

      <ReasonDialog
        open={dialog?.kind === 'restrict'}
        title={
          dialog?.kind === 'restrict' && dialog.req.activity_hidden
            ? t('admin.dataRequests.showActivityTitle')
            : t('admin.dataRequests.hideActivityTitle')
        }
        description={
          dialog?.kind === 'restrict' && dialog.req.activity_hidden
            ? t('admin.dataRequests.showActivityDescription')
            : t('admin.dataRequests.hideActivityDescription')
        }
        confirmLabel={
          dialog?.kind === 'restrict' && dialog.req.activity_hidden
            ? t('admin.dataRequests.showActivity')
            : t('admin.dataRequests.hideActivity')
        }
        reasonLabel={t('admin.dataRequests.restrictReason')}
        onCancel={() => setDialog(null)}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'restrict') return;
          await api.setDataRequestRestriction(dialog.req.id, !dialog.req.activity_hidden, reason);
          await done(
            dialog.req.activity_hidden
              ? t('admin.dataRequests.activityShown')
              : t('admin.dataRequests.activityHidden'),
          );
        }}
      />

      <ReasonDialog
        open={dialog?.kind === 'rectify'}
        title={t('admin.dataRequests.rectifyTitle')}
        description={t('admin.dataRequests.rectifyDescription')}
        confirmLabel={t('admin.dataRequests.rectifyConfirm')}
        reasonLabel={t('admin.dataRequests.rectifyReason')}
        valid={personId.trim() === '' || /^\d+$/.test(personId.trim())}
        onCancel={() => setDialog(null)}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'rectify') return;
          await api.rectifyPersonLink(dialog.req.id, reason, personId.trim() || undefined);
          await done(t('admin.dataRequests.rectified'));
        }}
      >
        <TextInput
          label={t('admin.dataRequests.rectifyPerson')}
          hint={t('admin.dataRequests.rectifyPersonHint')}
          inputMode="numeric"
          value={personId}
          onChange={(e) => setPersonId(e.target.value)}
        />
      </ReasonDialog>

      <ReasonDialog
        open={dialog?.kind === 'run'}
        title={
          dialog?.kind === 'run' && dialog.req.kind === 'erasure'
            ? t('admin.dataRequests.runErasureTitle')
            : t('admin.dataRequests.runExportTitle')
        }
        description={
          dialog?.kind === 'run' && dialog.req.kind === 'erasure'
            ? t('admin.dataRequests.runErasureDescription', { name: dialog.req.username })
            : t('admin.dataRequests.runExportDescription')
        }
        confirmLabel={
          dialog?.kind === 'run' && dialog.req.kind === 'erasure'
            ? t('admin.dataRequests.runErasureConfirm')
            : t('admin.dataRequests.runExportConfirm')
        }
        danger={dialog?.kind === 'run' && dialog.req.kind === 'erasure'}
        reasonOptional
        onCancel={() => setDialog(null)}
        onConfirm={async () => {
          if (dialog?.kind !== 'run') return;
          await api.runDataRequest(dialog.req.id, dialog.req.kind === 'erasure' && deleteMessages);
          await done(t('admin.dataRequests.ran'));
        }}
      >
        {dialog?.kind === 'run' && dialog.req.kind === 'erasure' && (
          <>
            <Checkbox
              label={t('admin.dataRequests.run.deleteMessages')}
              checked={deleteMessages}
              onChange={(e) => setDeleteMessages(e.target.checked)}
            />
            <p className="ss-sub">{t('admin.dataRequests.runErasureKept')}</p>
            <ul>
              {catalog.map((c) => (
                <li key={c.id}>
                  <strong>{c.label}.</strong> {c.reason}
                </li>
              ))}
            </ul>
          </>
        )}
      </ReasonDialog>
    </>
  );
}
