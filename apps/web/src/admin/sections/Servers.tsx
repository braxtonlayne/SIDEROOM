// Servers admin (ADM-COM-002): search every server, see its metadata, transfer ownership or delete it. All actions
// need MANAGE_GUILDS_GLOBAL and a reason, and run as the owner through the normal chat stack (members hear the
// usual events). The Staff Server cannot be changed here; the server refuses it, and the buttons stay disabled.
import { Badge, Button, Drawer, Notice, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { DataTable, type Column } from '../DataTable';
import { Async, ReasonDialog, SectionHeader, fmtMs, usePaged, useResource } from '../ui';

export function ServersSection() {
  const t = useT();
  const [draft, setDraft] = useState('');
  const [q, setQ] = useState('');
  const [open, setOpen] = useState<string | null>(null);
  const paged = usePaged<api.AdminGuild>(`admin-guilds:${q}`, async (after) => {
    const p = await api.listAdminGuilds({ ...(q ? { q } : {}), after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });
  const columns: Column<api.AdminGuild>[] = [
    {
      id: 'name',
      header: t('admin.servers.colName'),
      locked: true,
      csv: (g) => g.name,
      cell: (g) => (
        <>
          <strong>{g.name}</strong>{' '}
          {g.is_staff_guild && <Badge>{t('admin.servers.staffServer')}</Badge>}
          {g.deleted && <Badge variant="danger">{t('admin.servers.deleted')}</Badge>}
        </>
      ),
    },
    {
      id: 'owner',
      header: t('admin.servers.colOwner'),
      csv: (g) => g.owner_username ?? g.owner_id,
      cell: (g) => (g.owner_username ? `@${g.owner_username}` : g.owner_id),
    },
    {
      id: 'members',
      header: t('admin.servers.colMembers'),
      csv: (g) => g.member_count,
      cell: (g) => g.member_count,
    },
    {
      id: 'channels',
      header: t('admin.servers.colChannels'),
      csv: (g) => g.channel_count,
      cell: (g) => g.channel_count,
    },
    {
      id: 'created',
      header: t('admin.servers.colCreated'),
      csv: (g) => new Date(g.created_at_ms).toISOString(),
      cell: (g) => fmtMs(g.created_at_ms),
    },
    {
      id: 'id',
      header: t('admin.servers.colId'),
      defaultHidden: true,
      csv: (g) => g.id,
      cell: (g) => <span className="adm-mono">{g.id}</span>,
    },
  ];
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setQ(draft.trim());
  };
  return (
    <>
      <SectionHeader
        title={t('admin.servers.title')}
        description={t('admin.servers.description')}
      />
      <DataTable
        tableId="servers"
        label={t('admin.servers.tableLabel')}
        columns={columns}
        paged={paged}
        rowKey={(g) => g.id}
        emptyTitle={t('admin.servers.emptyTitle')}
        emptyDescription={t('admin.servers.emptyDescription')}
        actions={(g) => (
          <Button
            size="sm"
            variant="secondary"
            aria-label={t('admin.servers.viewAria', { name: g.name })}
            onClick={() => setOpen(g.id)}
          >
            {t('admin.servers.view')}
          </Button>
        )}
        filters={
          <form
            className="ss-toolbar"
            onSubmit={submit}
            role="search"
            aria-label={t('admin.servers.searchForm')}
          >
            <div className="ss-search">
              <TextInput
                label={t('admin.servers.searchLabel')}
                type="search"
                value={draft}
                placeholder={t('admin.servers.searchPlaceholder')}
                onChange={(e) => setDraft(e.target.value)}
              />
            </div>
            <Button type="submit">{t('admin.servers.search')}</Button>
          </form>
        }
      />
      {open && (
        <ServerDrawer id={open} onClose={() => setOpen(null)} onChanged={() => paged.reload()} />
      )}
    </>
  );
}

function ServerDrawer({
  id,
  onClose,
  onChanged,
}: {
  id: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const t = useT();
  const res = useResource(`admin-guild:${id}`, () => api.getAdminGuild(id));
  const [pending, setPending] = useState<'transfer' | 'delete' | null>(null);
  const [newOwner, setNewOwner] = useState('');
  const [typed, setTyped] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <Drawer
      open
      onClose={onClose}
      title={res.data ? res.data.name : t('admin.servers.drawerTitle')}
    >
      <Async res={res} label={t('admin.servers.loadingServer')}>
        {(g) => (
          <div className="adm-form">
            {notice && (
              <Notice variant="success" role="status">
                {notice}
              </Notice>
            )}
            <dl className="adm-kv">
              <dt>{t('admin.servers.dtId')}</dt>
              <dd className="adm-mono">{g.id}</dd>
              <dt>{t('admin.servers.colOwner')}</dt>
              <dd>
                {g.owner_username ? `@${g.owner_username} ` : ''}
                <span className="adm-mono">{g.owner_id}</span>
              </dd>
              <dt>{t('admin.servers.colMembers')}</dt>
              <dd>{g.member_count}</dd>
              <dt>{t('admin.servers.colChannels')}</dt>
              <dd>{g.channel_count}</dd>
              <dt>{t('admin.servers.colCreated')}</dt>
              <dd>{fmtMs(g.created_at_ms)}</dd>
              <dt>{t('admin.servers.dtCommunity')}</dt>
              <dd>{g.community_enabled ? t('admin.accounts.on') : t('admin.accounts.off')}</dd>
              {g.deleted && (
                <>
                  <dt>{t('admin.accounts.dtState')}</dt>
                  <dd>{t('admin.servers.deleted')}</dd>
                </>
              )}
            </dl>
            {g.is_staff_guild && (
              <Notice variant="info">{t('admin.servers.staffServerNote')}</Notice>
            )}
            <div className="adm-actions">
              <Button
                variant="secondary"
                disabled={g.is_staff_guild || g.deleted}
                onClick={() => {
                  setNewOwner('');
                  setPending('transfer');
                }}
              >
                {t('admin.servers.transfer')}
              </Button>
              <Button
                variant="danger"
                disabled={g.is_staff_guild || g.deleted}
                onClick={() => {
                  setTyped('');
                  setPending('delete');
                }}
              >
                {t('admin.servers.delete')}
              </Button>
            </div>

            <ReasonDialog
              open={pending === 'transfer'}
              title={t('admin.servers.transferTitle', { name: g.name })}
              description={t('admin.servers.transferDescription')}
              confirmLabel={t('admin.servers.transfer')}
              valid={/^\d+$/.test(newOwner.trim()) && newOwner.trim() !== g.owner_id}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                await api.transferGuild(id, newOwner.trim(), reason);
                setPending(null);
                setNotice(t('admin.servers.transferred'));
                await res.reload();
                onChanged();
              }}
            >
              <TextInput
                label={t('admin.servers.newOwnerId')}
                inputMode="numeric"
                value={newOwner}
                onChange={(e) => setNewOwner(e.target.value)}
              />
            </ReasonDialog>
            <ReasonDialog
              open={pending === 'delete'}
              danger
              title={t('admin.servers.deleteTitle', { name: g.name })}
              description={t('admin.servers.deleteDescription')}
              confirmLabel={t('admin.servers.delete')}
              valid={typed.trim() === g.name}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                await api.deleteAdminGuild(id, reason);
                setPending(null);
                setNotice(t('admin.servers.wasDeleted'));
                await res.reload();
                onChanged();
              }}
            >
              <TextInput
                label={t('admin.servers.typeName', { name: g.name })}
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
              />
            </ReasonDialog>
          </div>
        )}
      </Async>
    </Drawer>
  );
}
