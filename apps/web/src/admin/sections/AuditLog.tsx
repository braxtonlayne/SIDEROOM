// Platform audit log (ADM-PLT-007): what staff changed, as a filterable, exportable table. These are the
// `admin_action` rows of the one hash-chained staff log, so the chain check lives on the Access log page.
import { Button, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { DataTable, type Column } from '../DataTable';
import { SectionHeader, fmtUs, usePaged } from '../ui';

const EMPTY = { action: '', actor: '', target: '' };

export function AuditLogSection() {
  const t = useT();
  const [draft, setDraft] = useState(EMPTY);
  const [applied, setApplied] = useState(EMPTY);
  const paged = usePaged<api.AccessLogEntry, number>(
    `audit:${applied.action}:${applied.actor}:${applied.target}`,
    async (before) => {
      const p = await api.listAccessLog({
        kind: 'admin_action',
        action: applied.action,
        actor_id: applied.actor,
        target_id: applied.target,
        before,
        limit: 50,
      });
      return { items: p.items, next: p.next_before };
    },
  );
  const columns: Column<api.AccessLogEntry>[] = [
    { id: 'seq', header: '#', csv: (e) => e.seq, cell: (e) => e.seq },
    {
      id: 'when',
      header: t('admin.auditLog.colWhen'),
      csv: (e) => new Date(e.at_us / 1000).toISOString(),
      cell: (e) => fmtUs(e.at_us),
    },
    {
      id: 'actor',
      header: t('admin.auditLog.colActor'),
      csv: (e) => e.actor_id,
      cell: (e) => <span className="adm-mono">{e.actor_id}</span>,
    },
    {
      id: 'action',
      header: t('admin.auditLog.colAction'),
      locked: true,
      csv: (e) => e.action,
      cell: (e) => e.action,
    },
    {
      id: 'target',
      header: t('admin.auditLog.colTarget'),
      csv: (e) => (e.target_id ? `${e.target_kind ?? ''} ${e.target_id}`.trim() : ''),
      cell: (e) =>
        e.target_id ? (
          <>
            {e.target_kind} <span className="adm-mono">{e.target_id}</span>
          </>
        ) : null,
    },
    {
      id: 'reason',
      header: t('admin.auditLog.colReason'),
      csv: (e) => e.reason,
      cell: (e) => e.reason,
    },
    {
      id: 'ip',
      header: t('admin.auditLog.colIp'),
      defaultHidden: true,
      csv: (e) => e.ip,
      cell: (e) => <span className="adm-mono">{e.ip}</span>,
    },
  ];
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setApplied({
      action: draft.action.trim(),
      actor: draft.actor.trim(),
      target: draft.target.trim(),
    });
  };
  return (
    <>
      <SectionHeader
        title={t('admin.auditLog.title')}
        description={t('admin.auditLog.description')}
      />
      <DataTable
        tableId="platform-audit-log"
        label={t('admin.auditLog.tableLabel')}
        columns={columns}
        paged={paged}
        rowKey={(e) => String(e.seq)}
        emptyTitle={t('admin.auditLog.emptyTitle')}
        emptyDescription={t('admin.auditLog.emptyDescription')}
        filters={
          <form
            className="ss-toolbar"
            role="search"
            aria-label={t('admin.auditLog.filterForm')}
            onSubmit={submit}
          >
            <TextInput
              label={t('admin.auditLog.filterAction')}
              placeholder={t('admin.auditLog.actionPlaceholder')}
              value={draft.action}
              onChange={(e) => setDraft({ ...draft, action: e.target.value })}
            />
            <TextInput
              label={t('admin.auditLog.filterActor')}
              inputMode="numeric"
              value={draft.actor}
              onChange={(e) => setDraft({ ...draft, actor: e.target.value })}
            />
            <TextInput
              label={t('admin.auditLog.filterTarget')}
              inputMode="numeric"
              value={draft.target}
              onChange={(e) => setDraft({ ...draft, target: e.target.value })}
            />
            <Button type="submit">{t('admin.auditLog.apply')}</Button>
          </form>
        }
      />
    </>
  );
}
