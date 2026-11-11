// Applications tab of the Review hub (D-587): every developer application for REVIEW_APPS holders, with search,
// a release-state filter, and Suspend / Unsuspend (the D-581 second factor prompts through withReauth). One
// application at a time; there is no bulk action. The owner is sent a system message by the server.
import { Badge, Button, Notice, Select, TextInput, type BadgeVariant } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import { DataTable, type Column } from '../DataTable';
import { ReasonDialog, fmtMs, usePaged } from '../ui';

const PAGE = 50;

const STATE_OPTIONS: { value: string; label: MessageKey }[] = [
  { value: '', label: 'admin.applications.stateAny' },
  { value: 'development', label: 'admin.applications.stateDevelopment' },
  { value: 'in_review', label: 'admin.applications.stateInReview' },
  { value: 'released', label: 'admin.applications.stateReleased' },
  { value: 'suspended', label: 'admin.applications.stateSuspended' },
];

const STATE_LABEL: Record<string, MessageKey> = {
  development: 'admin.applications.stateDevelopment',
  in_review: 'admin.applications.stateInReview',
  released: 'admin.applications.stateReleased',
  suspended: 'admin.applications.stateSuspended',
};

const STATE_VARIANT: Record<string, BadgeVariant> = {
  development: 'neutral',
  in_review: 'accent',
  released: 'success',
  suspended: 'danger',
};

type Pending = { app: api.AdminApp; suspend: boolean } | null;

export function Applications() {
  const t = useT();
  const [draft, setDraft] = useState({ q: '', state: '' });
  const [applied, setApplied] = useState({ q: '', state: '' });
  const [pending, setPending] = useState<Pending>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const paged = usePaged<api.AdminApp>(
    `admin-apps:${applied.q}:${applied.state}`,
    async (before) => {
      const items = await api.listAdminApps({
        ...(applied.q ? { q: applied.q } : {}),
        ...(applied.state ? { state: applied.state } : {}),
        before,
      });
      return { items, next: items.length >= PAGE ? items[items.length - 1]!.id : null };
    },
  );
  const ownerName = (a: api.AdminApp) => (a.owner.username ? `@${a.owner.username}` : a.owner.id);
  const rowActions = (a: api.AdminApp) =>
    a.release_state === 'suspended' ? (
      <Button
        size="sm"
        variant="secondary"
        aria-label={t('admin.applications.unsuspendAria', { name: a.name })}
        onClick={() => setPending({ app: a, suspend: false })}
      >
        {t('admin.applications.unsuspend')}
      </Button>
    ) : (
      <Button
        size="sm"
        variant="danger"
        aria-label={t('admin.applications.suspendAria', { name: a.name })}
        onClick={() => setPending({ app: a, suspend: true })}
      >
        {t('admin.applications.suspend')}
      </Button>
    );
  const columns: Column<api.AdminApp>[] = [
    {
      id: 'name',
      header: t('admin.applications.colName'),
      locked: true,
      csv: (a) => a.name,
      cell: (a) => (
        <>
          <strong>{a.name}</strong>
          <div className="ss-meta">{ownerName(a)}</div>
          {a.release_state === 'suspended' && (
            <div className="ss-meta">
              {a.suspended_reason && (
                <div>{t('admin.applications.reasonLine', { reason: a.suspended_reason })}</div>
              )}
              {a.suspended_at && (
                <div>
                  {a.suspended_by?.username
                    ? t('admin.applications.suspendedBy', {
                        name: a.suspended_by.username,
                        date: fmtMs(Date.parse(a.suspended_at)),
                      })
                    : t('admin.applications.suspendedByUnknown', {
                        date: fmtMs(Date.parse(a.suspended_at)),
                      })}
                </div>
              )}
            </div>
          )}
          <div className="adm-app-actions">{rowActions(a)}</div>
        </>
      ),
    },
    {
      id: 'owner',
      header: t('admin.applications.colOwner'),
      defaultHidden: true,
      csv: ownerName,
      cell: ownerName,
    },
    {
      id: 'state',
      header: t('admin.applications.colState'),
      locked: true,
      csv: (a) => a.release_state,
      cell: (a) => (
        <Badge variant={STATE_VARIANT[a.release_state] ?? 'neutral'}>
          {STATE_LABEL[a.release_state] ? t(STATE_LABEL[a.release_state]!) : a.release_state}
        </Badge>
      ),
    },
    {
      id: 'created',
      header: t('admin.applications.colCreated'),
      defaultHidden: true,
      csv: (a) => a.created_at,
      cell: (a) => fmtMs(Date.parse(a.created_at)),
    },
    {
      id: 'id',
      header: t('admin.applications.colId'),
      defaultHidden: true,
      csv: (a) => a.id,
      cell: (a) => <span className="adm-mono">{a.id}</span>,
    },
  ];
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setApplied({ q: draft.q.trim(), state: draft.state });
  };
  const name = pending?.app.name ?? '';
  return (
    <>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <DataTable
        tableId="applications"
        label={t('admin.applications.tableLabel')}
        columns={columns}
        paged={paged}
        rowKey={(a) => a.id}
        emptyTitle={t('admin.applications.emptyTitle')}
        emptyDescription={t('admin.applications.emptyDescription')}
        filters={
          <form
            className="ss-toolbar"
            onSubmit={submit}
            role="search"
            aria-label={t('admin.applications.searchForm')}
          >
            <div className="ss-search">
              <TextInput
                label={t('admin.applications.searchLabel')}
                type="search"
                maxLength={100}
                value={draft.q}
                placeholder={t('admin.applications.searchPlaceholder')}
                onChange={(e) => setDraft({ ...draft, q: e.target.value })}
              />
            </div>
            <Select
              aria-label={t('admin.applications.filterByState')}
              options={STATE_OPTIONS.map((o) => ({ value: o.value, label: t(o.label) }))}
              value={draft.state}
              onValueChange={(state) => {
                setDraft({ ...draft, state });
                setApplied({ q: draft.q.trim(), state });
              }}
            />
            <Button type="submit">{t('admin.applications.search')}</Button>
          </form>
        }
      />
      <ReasonDialog
        open={pending !== null}
        title={
          pending?.suspend
            ? t('admin.applications.suspendTitle', { name })
            : t('admin.applications.unsuspendTitle', { name })
        }
        description={
          pending?.suspend
            ? t('admin.applications.suspendDescription')
            : t('admin.applications.unsuspendDescription')
        }
        confirmLabel={
          pending?.suspend
            ? t('admin.applications.suspendConfirm')
            : t('admin.applications.unsuspendConfirm')
        }
        danger={pending?.suspend === true}
        reasonOptional={pending?.suspend === false}
        reasonLabel={
          pending?.suspend
            ? t('admin.applications.suspendReason')
            : t('admin.applications.unsuspendReason')
        }
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          if (!pending) return;
          if (pending.suspend) await api.suspendApp(pending.app.id, reason);
          else await api.unsuspendApp(pending.app.id, reason);
          const done = pending;
          setPending(null);
          setNotice(
            t(done.suspend ? 'admin.applications.suspended' : 'admin.applications.unsuspended', {
              name: done.app.name,
            }),
          );
          paged.reload();
        }}
      />
    </>
  );
}
