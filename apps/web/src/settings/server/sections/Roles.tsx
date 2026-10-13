import {
  Button,
  Combobox,
  ColorPicker,
  ConfirmDialog,
  Notice,
  PermissionMatrix,
  Switch,
  Tabs,
  TextInput,
  type TriState,
} from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useMembers, useStore } from '../../../store';
import * as api from '../api';
import {
  hexFromInt,
  intFromHex,
  useDraft,
  useGuildEvents,
  useResource,
  useSettingsForm,
  type Viewer,
} from '../hooks';
import { PERM, bitsToValues, has, permissionGroups, toBig } from '../permissions';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, ColorDot, SectionHeader, useAction } from '../ui';

const GROUPS = permissionGroups();

/** Why the viewer cannot edit this role, or undefined when they can (hierarchy rules, docs/07). */
export function roleEditReason(
  viewer: Viewer,
  role: api.RoleObject,
  guildId: string,
): string | undefined {
  if (!viewer.can('MANAGE_ROLES')) return 'You need the Manage Roles permission.';
  if (role.id === guildId) return undefined; // @everyone: permissions only
  if (viewer.isOwner) return undefined;
  if (viewer.topPosition <= role.position)
    return 'You cannot edit this role because it is at or above your highest role.';
  return undefined;
}

export function RolesSection({ guildId, viewer }: SectionProps) {
  const res = useResource(`roles:${guildId}`, () => api.listRoles(guildId));
  useGuildEvents(
    guildId,
    ['GUILD_ROLE_CREATE', 'GUILD_ROLE_UPDATE', 'GUILD_ROLE_DELETE'],
    () => void res.reload(),
  );
  return (
    <>
      <SectionHeader
        title="Roles"
        description="Use roles to group your members and assign permissions. Roles higher in the list outrank the ones below."
      />
      <Async res={res} label="Loading roles">
        {(roles) => (
          <RolesEditor
            guildId={guildId}
            viewer={viewer}
            roles={roles}
            reload={res.reload}
            set={res.set}
          />
        )}
      </Async>
    </>
  );
}

function RolesEditor({
  guildId,
  viewer,
  roles,
  reload,
  set,
}: {
  guildId: string;
  viewer: Viewer;
  roles: api.RoleObject[];
  reload: () => Promise<void>;
  set: (r: api.RoleObject[]) => void;
}) {
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editorDirty, setEditorDirty] = useState(false);
  const [blocked, setBlocked] = useState(false);
  const action = useAction();
  const { members } = useMembers(guildId);
  const counts = useMemo(() => {
    const c = new Map<string, number>();
    for (const m of members) for (const r of m.member.roles) c.set(r, (c.get(r) ?? 0) + 1);
    return c;
  }, [members]);

  const everyone = roles.find((r) => r.id === guildId);
  const ordered = useMemo(
    () => roles.filter((r) => r.id !== guildId).sort((a, b) => b.position - a.position),
    [roles, guildId],
  );
  const shown = query.trim()
    ? ordered.filter((r) => r.name.toLowerCase().includes(query.trim().toLowerCase()))
    : ordered;
  const selected =
    roles.find((r) => r.id === (selectedId ?? ordered[0]?.id ?? guildId)) ?? ordered[0] ?? everyone;

  const select = (id: string) => {
    if (editorDirty && id !== selected?.id) {
      setBlocked(true);
      return;
    }
    setBlocked(false);
    setSelectedId(id);
  };

  const create = async () => {
    const made = await action.run(() => api.createRole(guildId, { name: 'new role' }));
    if (made) {
      await reload();
      setBlocked(false);
      setEditorDirty(false);
      setSelectedId(made.id);
    }
  };

  /** Move `id` to display index `to`: rotate the position values of the roles in between. */
  const move = async (id: string, to: number) => {
    const from = ordered.findIndex((r) => r.id === id);
    if (from < 0 || to < 0 || to >= ordered.length || from === to) return;
    const lo = Math.min(from, to);
    const hi = Math.max(from, to);
    const slice = ordered.slice(lo, hi + 1);
    const positions = slice.map((r) => r.position);
    const reordered = [...ordered];
    const [item] = reordered.splice(from, 1);
    reordered.splice(to, 0, item!);
    const moves = reordered
      .slice(lo, hi + 1)
      .map((r, i) => ({ id: r.id, position: positions[i]! }))
      .filter((m) => roles.find((r) => r.id === m.id)?.position !== m.position);
    if (moves.length === 0) return;
    const prev = roles;
    set(
      roles.map((r) => ({
        ...r,
        position: moves.find((m) => m.id === r.id)?.position ?? r.position,
      })),
    );
    const ok = await action.run(() => api.reorderRoles(guildId, moves));
    if (!ok) set(prev);
    await reload();
  };

  const moveReason = (r: api.RoleObject, targetIndex: number): string | undefined => {
    const own = roleEditReason(viewer, r, guildId);
    if (own) return own;
    const target = ordered[targetIndex];
    if (target && !viewer.isOwner && viewer.topPosition <= target.position)
      return 'You cannot move a role to or above your own highest role.';
    return undefined;
  };

  const [dragId, setDragId] = useState<string | null>(null);

  return (
    <div className="ss-split">
      <div>
        <div className="ss-toolbar">
          <div className="ss-search">
            <TextInput
              label="Search roles"
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search roles"
            />
          </div>
          <Button onClick={() => void create()} loading={action.busy}>
            Create Role
          </Button>
        </div>
        <ActionStatus error={action.error} />
        {blocked && (
          <Notice variant="warning" onDismiss={() => setBlocked(false)}>
            Save or reset your changes before selecting another role.
          </Notice>
        )}
        <ul className="ss-list" aria-label="Roles">
          {shown.map((r) => {
            const idx = ordered.indexOf(r);
            const canUp = idx > 0 && !moveReason(r, idx - 1) && !query.trim();
            const canDown = idx < ordered.length - 1 && !moveReason(r, idx + 1) && !query.trim();
            return (
              <li
                key={r.id}
                className={`ss-row${dragId === r.id ? ' ss-dragging' : ''}`}
                draggable={!query.trim() && !roleEditReason(viewer, r, guildId)}
                onDragStart={(e) => {
                  setDragId(r.id);
                  e.dataTransfer.effectAllowed = 'move';
                  e.dataTransfer.setData('text/plain', r.id);
                }}
                onDragOver={(e) => {
                  if (dragId) e.preventDefault();
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  const id = dragId ?? e.dataTransfer.getData('text/plain');
                  setDragId(null);
                  if (id && id !== r.id) {
                    const target = moveReason(r, idx);
                    if (!target) void move(id, idx);
                  }
                }}
                onDragEnd={() => setDragId(null)}
              >
                <ColorDot color={r.color} />
                <button
                  type="button"
                  className="ss-grow ss-name ss-link"
                  aria-current={selected?.id === r.id ? 'true' : undefined}
                  onClick={() => select(r.id)}
                >
                  {r.name}
                </button>
                <span className="ss-meta">{counts.get(r.id) ?? 0} members</span>
                {!query.trim() && (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      iconOnly
                      aria-label={`Move ${r.name} up`}
                      disabled={!canUp}
                      onClick={() => void move(r.id, idx - 1)}
                    >
                      <span aria-hidden="true">{'↑'}</span>
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      iconOnly
                      aria-label={`Move ${r.name} down`}
                      disabled={!canDown}
                      onClick={() => void move(r.id, idx + 1)}
                    >
                      <span aria-hidden="true">{'↓'}</span>
                    </Button>
                  </>
                )}
              </li>
            );
          })}
          {everyone && (
            <li className="ss-row">
              <span className="ss-grow">
                <button
                  type="button"
                  className="ss-name ss-link"
                  aria-current={selected?.id === everyone.id ? 'true' : undefined}
                  onClick={() => select(everyone.id)}
                >
                  Default permissions
                </button>
                <span className="ss-meta"> @everyone</span>
              </span>
              <span className="ss-meta">All members</span>
            </li>
          )}
        </ul>
        {shown.length === 0 && <p className="ss-na">No roles match your search.</p>}
      </div>
      {selected && (
        <RoleEditor
          key={selected.id}
          guildId={guildId}
          viewer={viewer}
          role={selected}
          onDirty={setEditorDirty}
          onChanged={reload}
          onDeleted={() => {
            setSelectedId(null);
            setEditorDirty(false);
          }}
        />
      )}
    </div>
  );
}

interface RoleFields {
  name: string;
  color: number;
  hoist: boolean;
  mentionable: boolean;
  permissions: string;
}

function RoleEditor({
  guildId,
  viewer,
  role,
  onDirty,
  onChanged,
  onDeleted,
}: {
  guildId: string;
  viewer: Viewer;
  role: api.RoleObject;
  onDirty: (d: boolean) => void;
  onChanged: () => Promise<void>;
  onDeleted: () => void;
}) {
  const isEveryone = role.id === guildId;
  const reason = roleEditReason(viewer, role, guildId);
  const locked = !!reason || (role.managed && !isEveryone);
  const base = useMemo<RoleFields>(
    () => ({
      name: isEveryone ? '@everyone' : role.name,
      color: role.color,
      hoist: role.hoist,
      mentionable: role.mentionable,
      permissions: role.permissions,
    }),
    [role, isEveryone],
  );
  const draft = useDraft(base);
  const action = useAction();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const name = draft.value.name.trim();
  const nameError =
    !isEveryone && draft.changed.name !== undefined && (name.length < 1 || name.length > 100)
      ? 'Role name must be between 1 and 100 characters.'
      : undefined;

  const save = useCallback(async () => {
    if (nameError || locked) return;
    const c = draft.changed;
    const body: api.RoleBody = {};
    if (c.name !== undefined && !isEveryone) body.name = name;
    if (c.color !== undefined) body.color = c.color;
    if (c.hoist !== undefined) body.hoist = c.hoist;
    if (c.mentionable !== undefined) body.mentionable = c.mentionable;
    if (c.permissions !== undefined) body.permissions = c.permissions;
    const next = await action.run(() => api.modifyRole(guildId, role.id, body), 'Role saved.');
    if (next) {
      draft.reset();
      await onChanged();
    }
  }, [nameError, locked, draft, isEveryone, name, action, guildId, role.id, onChanged]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });
  useEffect(() => onDirty(draft.dirty), [onDirty, draft.dirty]);

  const perms = toBig(draft.value.permissions);
  const values = useMemo(() => bitsToValues(perms), [perms]);
  const setPerm = (id: string, v: TriState) => {
    const bit = PERM[id]!;
    draft.set('permissions', String(v === 'allow' ? perms | bit : perms & ~bit));
  };
  const lockedReason = (id: string) =>
    locked
      ? 'Read only.'
      : !viewer.canGrant(PERM[id]!)
        ? 'You cannot change a permission you do not have.'
        : undefined;

  const remove = async () => {
    const ok = await action.run(async () => {
      await api.deleteRole(guildId, role.id);
      return true;
    });
    if (ok) {
      setConfirmDelete(false);
      onDeleted();
      await onChanged();
    }
  };

  const tabs = [
    ...(!isEveryone
      ? [
          {
            id: 'display',
            label: 'Display',
            content: (
              <div className="ss-form">
                <TextInput
                  label="Role name"
                  value={draft.value.name}
                  maxLength={100}
                  disabled={locked}
                  error={nameError}
                  onChange={(e) => draft.set('name', e.target.value)}
                />
                <div>
                  <span className="ss-field-label">Role color</span>
                  <ColorPicker
                    label="Role color"
                    value={draft.value.color ? hexFromInt(draft.value.color) : '#99aab5'}
                    onValueChange={(hex) => !locked && draft.set('color', intFromHex(hex))}
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={locked || draft.value.color === 0}
                    onClick={() => draft.set('color', 0)}
                  >
                    Use default color
                  </Button>
                </div>
                <Switch
                  label="Display role members separately from online members"
                  checked={draft.value.hoist}
                  disabled={locked}
                  onChange={(e) => draft.set('hoist', e.target.checked)}
                />
                <Switch
                  label="Allow anyone to @mention this role"
                  checked={draft.value.mentionable}
                  disabled={locked}
                  onChange={(e) => draft.set('mentionable', e.target.checked)}
                />
              </div>
            ),
          },
        ]
      : []),
    {
      id: 'permissions',
      label: isEveryone ? 'Default permissions' : 'Permissions',
      content: (
        <div className="ss-form">
          {has(perms, PERM.ADMINISTRATOR!) && (
            <Notice variant="warning" title="Administrator">
              Members with this role have every permission and bypass channel overwrites. Only give
              it to people you trust.
            </Notice>
          )}
          <PermissionMatrix
            groups={GROUPS}
            values={values}
            mode="binary"
            lockedReason={lockedReason}
            onChange={(id, v) => !locked && setPerm(id, v)}
            toolbar={
              <Button
                size="sm"
                variant="secondary"
                disabled={locked || perms === 0n}
                onClick={() => draft.set('permissions', '0')}
              >
                Clear permissions
              </Button>
            }
          />
        </div>
      ),
    },
    ...(!isEveryone
      ? [
          {
            id: 'members',
            label: 'Manage Members',
            content: <RoleMembers guildId={guildId} viewer={viewer} role={role} />,
          },
        ]
      : []),
  ];

  return (
    <section aria-label={isEveryone ? 'Default permissions' : `Edit role ${role.name}`}>
      <h2 className="ss-title">{isEveryone ? 'Default permissions (@everyone)' : role.name}</h2>
      {reason && (
        <Notice variant="info" title="Read only" className="ss-status">
          {reason}
        </Notice>
      )}
      {role.managed && !isEveryone && (
        <Notice variant="info" className="ss-status">
          This role is managed by an integration. Its permissions cannot be edited.
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      <Tabs items={tabs} label="Role settings" className="ss-tabs" />
      {!isEveryone && !role.managed && (
        <div className="ss-actions" style={{ marginTop: 24 }}>
          <Button
            variant="danger"
            disabled={!!reason}
            onClick={() => setConfirmDelete(true)}
            aria-label={`Delete role ${role.name}`}
          >
            Delete role
          </Button>
        </div>
      )}
      <ConfirmDialog
        open={confirmDelete}
        danger
        title={`Delete ${role.name}`}
        description="Members lose this role and its permissions. This cannot be undone."
        confirmLabel="Delete role"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setConfirmDelete(false)}
      />
    </section>
  );
}

function RoleMembers({
  guildId,
  viewer,
  role,
}: {
  guildId: string;
  viewer: Viewer;
  role: api.RoleObject;
}) {
  const { members } = useMembers(guildId);
  const [query, setQuery] = useState('');
  const action = useAction();
  const reason =
    roleEditReason(viewer, role, guildId) ??
    (role.managed ? 'Managed roles cannot be assigned by hand.' : undefined);
  const holders = members.filter((m) => m.member.roles.includes(role.id));
  const others = members.filter((m) => !m.member.roles.includes(role.id));
  const nameOf = (m: (typeof members)[number]) =>
    m.member.nick || m.user?.global_name || m.user?.username || m.member.user_id;
  const shown = holders.filter((m) => nameOf(m).toLowerCase().includes(query.trim().toLowerCase()));

  const patchLocal = (userId: string, add: boolean) => {
    const m = members.find((x) => x.member.user_id === userId);
    if (!m || !m.user) return;
    const roles = add ? [...m.member.roles, role.id] : m.member.roles.filter((r) => r !== role.id);
    useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
      guild_id: guildId,
      user: m.user,
      nick: m.member.nick,
      roles,
    });
  };
  const assign = async (userId: string) => {
    const ok = await action.run(async () => {
      await api.addMemberRole(guildId, userId, role.id);
      return true;
    });
    if (ok) patchLocal(userId, true);
  };
  const unassign = async (userId: string) => {
    const ok = await action.run(async () => {
      await api.removeMemberRole(guildId, userId, role.id);
      return true;
    });
    if (ok) patchLocal(userId, false);
  };

  return (
    <div className="ss-form">
      {reason && <p className="ss-lock">{reason}</p>}
      <ActionStatus error={action.error} />
      <Combobox
        aria-label="Add a member to this role"
        placeholder="Add a member"
        disabled={!!reason}
        options={others.map((m) => ({ value: m.member.user_id, label: nameOf(m) }))}
        value={null}
        onValueChange={(id) => void assign(id)}
      />
      <TextInput
        label="Search members with this role"
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <ul className="ss-list" aria-label={`Members with ${role.name}`}>
        {shown.map((m) => (
          <li key={m.member.user_id} className="ss-row">
            <span className="ss-grow ss-name">{nameOf(m)}</span>
            <Button
              size="sm"
              variant="secondary"
              disabled={!!reason}
              aria-label={`Remove ${nameOf(m)} from ${role.name}`}
              onClick={() => void unassign(m.member.user_id)}
            >
              Remove
            </Button>
          </li>
        ))}
      </ul>
      {shown.length === 0 && <p className="ss-na">No members have this role.</p>}
    </div>
  );
}
