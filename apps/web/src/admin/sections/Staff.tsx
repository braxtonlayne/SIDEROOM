import { Badge, Button, Checkbox, ConfirmDialog, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
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
  const viewer = useAdminViewer();
  return (
    <fieldset className="adm-check-row" style={{ border: 0, padding: 0, margin: 0 }}>
      <legend className="ss-sub">Staff roles</legend>
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
                  ? 'The Owner role cannot be assigned. Use sr-cli staff transfer-owner.'
                  : 'Only the Owner, or someone who holds all of its permissions, can assign this role.'
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
        title="Staff & roles"
        description="Staff powers come from roles in the Staff Server and only apply to accounts with an active appointment. Removing someone takes effect on their next request."
        actions={<Button onClick={openAppoint}>Appoint staff</Button>}
      />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />
      <Async res={staff} label="Loading staff">
        {(list) => (
          <ul className="ss-list" aria-label="Staff">
            {list.map((m) => (
              <li key={m.account_id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {m.display_name || m.username} <span className="ss-meta">@{m.username}</span>{' '}
                    {m.is_owner && <Badge variant="accent">Owner</Badge>}
                  </div>
                  <div className="ss-meta">
                    {m.roles.length ? m.roles.map((r) => r.name).join(', ') : 'No roles'} ·
                    appointed {fmtMs(m.appointed_at_ms)}
                    {m.reason ? ` · “${m.reason}”` : ''}
                  </div>
                </div>
                {!m.is_owner && (
                  <>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={`Change roles for ${m.username}`}
                      onClick={() => openEdit(m)}
                    >
                      Roles
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={`Remove ${m.username} from staff`}
                      onClick={() => setRevoking(m)}
                    >
                      Remove
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
          Only the Owner can grant Read private content, Appoint staff, View access log or Operator.
          Ownership itself moves only with <code>sr-cli staff transfer-owner</code>.
        </p>
      )}

      <ReasonDialog
        open={appointing}
        title="Appoint staff"
        description="The account joins the Staff Server with these roles. Everything is logged."
        confirmLabel="Appoint"
        valid={/^\d+$/.test(accountId.trim()) && picked.length > 0}
        onCancel={() => setAppointing(false)}
        onConfirm={async (reason) => {
          await api.appoint(accountId.trim(), picked, reason);
          setAppointing(false);
          setNotice('Staff member appointed.');
          await staff.reload();
        }}
      >
        <TextInput
          label="Account id"
          hint="Find the id under Accounts."
          inputMode="numeric"
          value={accountId}
          onChange={(e) => setAccountId(e.target.value)}
        />
        <Async res={roles} label="Loading roles">
          {(rs) => <RolePicker roles={rs} selected={picked} onChange={setPicked} />}
        </Async>
      </ReasonDialog>

      <ReasonDialog
        open={!!editing}
        title={`Roles for @${editing?.username ?? ''}`}
        confirmLabel="Save roles"
        onCancel={() => setEditing(null)}
        onConfirm={async (reason) => {
          if (!editing) return;
          await api.setStaffRoles(editing.account_id, editRoles, reason);
          setEditing(null);
          setNotice('Roles updated.');
          await staff.reload();
        }}
      >
        <Async res={roles} label="Loading roles">
          {(rs) => <RolePicker roles={rs} selected={editRoles} onChange={setEditRoles} />}
        </Async>
      </ReasonDialog>

      <ConfirmDialog
        open={!!revoking}
        danger
        title={`Remove @${revoking?.username ?? ''} from staff`}
        description="Their appointment closes, their admin access ends on the next request and they leave the Staff Server."
        confirmLabel="Remove from staff"
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
                setNotice(`@${revoking.username} was removed from staff.`);
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
