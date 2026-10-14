import { Badge, Button, Checkbox, ConfirmDialog, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { canAssignRole, parsePerms, permNames } from '../permissions';
import {
  ActionStatus,
  Async,
  ReasonDialog,
  SectionHeader,
  fmtMs,
  useAction,
  useAdminViewer,
  useResource,
} from '../ui';

function RolePicker({
  roles,
  selected,
  onChange,
}: {
  roles: api.StaffRole[];
  selected: string[];
  onChange: (ids: string[]) => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  return (
    <fieldset className="adm-check-row" style={{ border: 0, padding: 0, margin: 0 }}>
      <legend className="ss-sub">{t('admin.staff.rolesLegend')}</legend>
      {roles.map((r) => {
        const bits = parsePerms(r.platform_permissions);
        const allowed = canAssignRole(viewer, bits);
        const checked = selected.includes(r.id);
        return (
          <Checkbox
            key={r.id}
            label={r.name}
            description={
              allowed
                ? permNames(bits).join(', ').toLowerCase().replace(/_/g, ' ')
                : viewer.isOwner
                  ? t('admin.staff.ownerRoleNote')
                  : t('admin.staff.roleNeedsOwner')
            }
            checked={checked}
            // A role already held stays removable even if the viewer could not grant it.
            disabled={!allowed && !checked}
            onChange={(e) =>
              onChange(e.target.checked ? [...selected, r.id] : selected.filter((x) => x !== r.id))
            }
          />
        );
      })}
    </fieldset>
  );
}

export function StaffSection() {
  const t = useT();
  const viewer = useAdminViewer();
  const staff = useResource('admin:staff', api.listStaff);
  const roles = useResource('admin:staff-roles', api.listStaffRoles);
  const [appointing, setAppointing] = useState(false);
  const [accountId, setAccountId] = useState('');
  const [picked, setPicked] = useState<string[]>([]);
  const [editing, setEditing] = useState<api.StaffMember | null>(null);
  const [editRoles, setEditRoles] = useState<string[]>([]);
  const [revoking, setRevoking] = useState<api.StaffMember | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();

  const openAppoint = () => {
    setAccountId('');
    setPicked([]);
    setAppointing(true);
  };
  const openEdit = (m: api.StaffMember) => {
    setEditing(m);
    setEditRoles(m.roles.map((r) => r.id));
  };

  return (
    <>
      <SectionHeader
        title={t('admin.staff.title')}
        description={t('admin.staff.description')}
        actions={<Button onClick={openAppoint}>{t('admin.staff.appointButton')}</Button>}
      />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />
      <Async res={staff} label={t('admin.staff.loading')}>
        {(list) => (
          <ul className="ss-list" aria-label={t('admin.staff.listLabel')}>
            {list.map((m) => (
              <li key={m.account_id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {m.display_name || m.username} <span className="ss-meta">@{m.username}</span>{' '}
                    {m.is_owner && <Badge variant="accent">{t('admin.staff.owner')}</Badge>}
                  </div>
                  <div className="ss-meta">
                    {m.roles.length
                      ? m.roles.map((r) => r.name).join(', ')
                      : t('admin.staff.noRoles')}{' '}
                    · {t('admin.staff.appointed', { when: fmtMs(m.appointed_at_ms) })}
                    {m.reason ? ` · ${t('admin.staff.reasonQuoted', { reason: m.reason })}` : ''}
                  </div>
                </div>
                {!m.is_owner && (
                  <>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('admin.staff.changeRolesAria', { username: m.username })}
                      onClick={() => openEdit(m)}
                    >
                      {t('admin.staff.roles')}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={t('admin.staff.removeAria', { username: m.username })}
                      onClick={() => setRevoking(m)}
                    >
                      {t('admin.staff.remove')}
                    </Button>
                  </>
                )}
              </li>
            ))}
          </ul>
        )}
      </Async>
      {!viewer.isOwner && (
        <p className="ss-sub" style={{ marginTop: 16 }}>
          {t('admin.staff.ownerOnlyNote')} {t('admin.staff.ownershipMovesWith')}{' '}
          <code>sr-cli staff transfer-owner</code>.
        </p>
      )}

      <ReasonDialog
        open={appointing}
        title={t('admin.staff.appointTitle')}
        description={t('admin.staff.appointDescription')}
        confirmLabel={t('admin.staff.appoint')}
        valid={/^\d+$/.test(accountId.trim()) && picked.length > 0}
        onCancel={() => setAppointing(false)}
        onConfirm={async (reason) => {
          await api.appoint(accountId.trim(), picked, reason);
          setAppointing(false);
          setNotice(t('admin.staff.appointedNotice'));
          await staff.reload();
        }}
      >
        <TextInput
          label={t('admin.staff.accountId')}
          hint={t('admin.staff.accountIdHint')}
          inputMode="numeric"
          value={accountId}
          onChange={(e) => setAccountId(e.target.value)}
        />
        <Async res={roles} label={t('admin.staff.loadingRoles')}>
          {(rs) => <RolePicker roles={rs} selected={picked} onChange={setPicked} />}
        </Async>
      </ReasonDialog>

      <ReasonDialog
        open={!!editing}
        title={t('admin.staff.rolesFor', { username: editing?.username ?? '' })}
        confirmLabel={t('admin.staff.saveRoles')}
        onCancel={() => setEditing(null)}
        onConfirm={async (reason) => {
          if (!editing) return;
          await api.setStaffRoles(editing.account_id, editRoles, reason);
          setEditing(null);
          setNotice(t('admin.staff.rolesUpdated'));
          await staff.reload();
        }}
      >
        <Async res={roles} label={t('admin.staff.loadingRoles')}>
          {(rs) => <RolePicker roles={rs} selected={editRoles} onChange={setEditRoles} />}
        </Async>
      </ReasonDialog>

      <ConfirmDialog
        open={!!revoking}
        danger
        title={t('admin.staff.removeTitle', { username: revoking?.username ?? '' })}
        description={t('admin.staff.removeDescription')}
        confirmLabel={t('admin.staff.removeConfirm')}
        loading={action.busy}
        onCancel={() => setRevoking(null)}
        onConfirm={() => {
          if (!revoking) return;
          void action
            .run(async () => {
              await api.revokeStaff(revoking.account_id);
              return true;
            })
            .then(async (ok) => {
              if (ok) {
                setNotice(t('admin.staff.removedNotice', { username: revoking.username }));
                setRevoking(null);
                await staff.reload();
              } else {
                setRevoking(null);
              }
            });
        }}
      />
    </>
  );
}
