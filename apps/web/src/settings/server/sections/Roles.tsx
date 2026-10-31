import {
  Badge,
  Button,
  Combobox,
  ColorPicker,
  ConfirmDialog,
  Notice,
  PermissionMatrix,
  Radio,
  RadioGroup,
  Switch,
  Tabs,
  TextInput,
  type TriState,
} from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { t as translate, useT } from '../../../i18n';
import { HOLOGRAPHIC, nameStyle, stopsOf } from '../../../lib/roleGradient';
import { cdnUrl, useMembers, useStore } from '../../../store';
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
import { PERM, PERMISSIONS, bitsToValues, has, permissionGroups, toBig } from '../permissions';
import { useRolePrefill, useWholeMirrors } from '../bridge/mirrors';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, ColorDot, NotAvailable, SectionHeader, useAction } from '../ui';
import { ImageField } from './ImageField';
import { StorageGrantsCard } from '../storage/GrantsCard';
import { ViewAsRoleDialog } from './ViewAsRole';

const GROUPS = permissionGroups();

/** Why the viewer cannot edit this role, or undefined when they can (hierarchy rules, docs/07). */
export function roleEditReason(
  viewer: Viewer,
  role: api.RoleObject,
  guildId: string,
): string | undefined {
  if (!viewer.can('MANAGE_ROLES')) return translate('settings.roles.needManageRoles');
  if (role.id === guildId) return undefined; // @everyone: permissions only
  if (viewer.isOwner) return undefined;
  if (viewer.topPosition <= role.position) return translate('settings.roles.aboveHighest');
  return undefined;
}

export function RolesSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const res = useResource(`roles:${guildId}`, () => api.listRoles(guildId));
  useGuildEvents(
    guildId,
    ['GUILD_ROLE_CREATE', 'GUILD_ROLE_UPDATE', 'GUILD_ROLE_DELETE'],
    () => void res.reload(),
  );
  return (
    <>
      <SectionHeader
        title={t('settings.roles.title')}
        description={t('settings.roles.description')}
      />
      <Async res={res} label={t('settings.roles.loading')}>
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
  const t = useT();
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editorDirty, setEditorDirty] = useState(false);
  const [blocked, setBlocked] = useState(false);
  const [viewAs, setViewAs] = useState(false);
  const action = useAction();
  const mirrors = useWholeMirrors(guildId);
  const prefillRole = useRolePrefill((s) =>
    s.prefill?.guildId === guildId ? s.prefill.roleId : null,
  );
  useEffect(() => {
    if (prefillRole) setSelectedId(prefillRole);
  }, [prefillRole]);
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
      return t('settings.roles.cannotMove');
    return undefined;
  };

  const [dragId, setDragId] = useState<string | null>(null);

  return (
    <div className="ss-split">
      <div>
        <div className="ss-toolbar">
          <div className="ss-search">
            <TextInput
              label={t('settings.roles.searchLabel')}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('settings.roles.searchLabel')}
            />
          </div>
          <Button variant="secondary" onClick={() => setViewAs(true)}>
            {t('settings.roles.viewAs')}
          </Button>
          <Button onClick={() => void create()} loading={action.busy}>
            {t('settings.roles.create')}
          </Button>
        </div>
        {viewAs && (
          <ViewAsRoleDialog guildId={guildId} roles={roles} onClose={() => setViewAs(false)} />
        )}
        <ActionStatus error={action.error} />
        {blocked && (
          <Notice variant="warning" onDismiss={() => setBlocked(false)}>
            {t('settings.roles.saveFirst')}
          </Notice>
        )}
        <ul className="ss-list" aria-label={t('settings.roles.listLabel')}>
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
                <RoleBadge role={r} guildId={guildId} />
                <button
                  type="button"
                  className="ss-grow ss-name ss-link"
                  aria-current={selected?.id === r.id ? 'true' : undefined}
                  onClick={() => select(r.id)}
                >
                  {r.name}
                </button>
                {mirrors?.roles.some((m) => m.role_id === r.id) && (
                  <Badge variant="accent">{t('settings.bridgeSync.role.mirrored')}</Badge>
                )}
                <span className="ss-meta">
                  {t('settings.roles.memberCount', { count: String(counts.get(r.id) ?? 0) })}
                </span>
                {!query.trim() && (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      iconOnly
                      aria-label={t('settings.roles.moveUp', { name: r.name })}
                      disabled={!canUp}
                      onClick={() => void move(r.id, idx - 1)}
                    >
                      <span aria-hidden="true">{'↑'}</span>
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      iconOnly
                      aria-label={t('settings.roles.moveDown', { name: r.name })}
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
                  {t('settings.roles.defaultPermissions')}
                </button>
                <span className="ss-meta"> {t('settings.roles.everyoneName')}</span>
              </span>
              <span className="ss-meta">{t('settings.roles.allMembers')}</span>
            </li>
          )}
        </ul>
        {shown.length === 0 && <p className="ss-na">{t('settings.roles.noMatch')}</p>}
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

/** How the role colour is built (J-08): one colour, two stops, or Discord's fixed holographic triple. */
type ColorStyle = 'solid' | 'gradient' | 'holographic';

export const colorStyleOf = (c: {
  secondary: number | null;
  tertiary: number | null;
}): ColorStyle => (c.tertiary != null ? 'holographic' : c.secondary != null ? 'gradient' : 'solid');

interface RoleFields {
  name: string;
  color: number;
  /** Second gradient stop, or null. */
  secondary: number | null;
  /** Third stop (holographic only), or null. */
  tertiary: number | null;
  hoist: boolean;
  mentionable: boolean;
  permissions: string;
  /** One emoji, or empty. */
  unicode_emoji: string;
  /** Stored hash, a data URI picked now, or null. */
  icon: string | null;
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
  const t = useT();
  const isEveryone = role.id === guildId;
  const reason = roleEditReason(viewer, role, guildId);
  const locked = !!reason || (role.managed && !isEveryone);
  const base = useMemo<RoleFields>(
    () => ({
      name: isEveryone ? '@everyone' : role.name,
      color: role.color,
      secondary: role.colors?.secondary_color ?? null,
      tertiary: role.colors?.tertiary_color ?? null,
      hoist: role.hoist,
      mentionable: role.mentionable,
      permissions: role.permissions,
      unicode_emoji: role.unicode_emoji ?? '',
      icon: role.icon ?? null,
    }),
    [role, isEveryone],
  );
  const draft = useDraft(base);
  const action = useAction();
  const mirrors = useWholeMirrors(guildId);
  const mirror = mirrors?.roles.find((m) => m.role_id === role.id);
  const prefill = useRolePrefill((s) => s.prefill);
  const setPrefill = useRolePrefill((s) => s.set);
  const [prefilled, setPrefilled] = useState(
    () => prefill?.guildId === guildId && prefill.roleId === role.id,
  );
  const [confirmDelete, setConfirmDelete] = useState(false);
  const name = draft.value.name.trim();
  const nameError =
    !isEveryone && draft.changed.name !== undefined && (name.length < 1 || name.length > 100)
      ? t('settings.roles.nameError')
      : undefined;

  const save = useCallback(async () => {
    if (nameError || locked) return;
    const c = draft.changed;
    const body: api.RoleBody = {};
    if (c.name !== undefined && !isEveryone) body.name = name;
    if (c.color !== undefined || c.secondary !== undefined || c.tertiary !== undefined) {
      const v = draft.value;
      body.colors = {
        primary_color: v.color,
        secondary_color: v.secondary,
        tertiary_color: v.secondary != null ? v.tertiary : null,
      };
    }
    if (c.hoist !== undefined) body.hoist = c.hoist;
    if (c.mentionable !== undefined) body.mentionable = c.mentionable;
    if (c.permissions !== undefined) body.permissions = c.permissions;
    if (c.unicode_emoji !== undefined) body.unicode_emoji = c.unicode_emoji.trim() || null;
    if (c.icon !== undefined) body.icon = c.icon;
    const next = await action.run(
      () => api.modifyRole(guildId, role.id, body),
      t('settings.roles.saved'),
    );
    if (next) {
      draft.reset();
      await onChanged();
    }
  }, [nameError, locked, draft, isEveryone, name, action, guildId, role.id, onChanged, t]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });
  useEffect(() => onDirty(draft.dirty), [onDirty, draft.dirty]);

  // "Apply on SIDE ROOM" from the conflict log: Discord's attempted permissions fill the form; nothing is saved.
  const prefillKey =
    prefill && prefill.guildId === guildId && prefill.roleId === role.id ? prefill : null;
  useEffect(() => {
    if (!prefillKey) return;
    const asked = new Set(prefillKey.permissions);
    let bits = toBig(role.permissions);
    for (const p of PERMISSIONS) {
      if (p.bit >= 56) continue;
      const bit = PERM[p.id]!;
      bits = asked.has(p.id) ? bits | bit : bits & ~bit;
    }
    draft.set('permissions', String(bits));
    setPrefilled(true);
    setPrefill(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefillKey]);
  const groups = useMemo(
    () =>
      mirror
        ? GROUPS.map((g) =>
            g.id === 'sideroom' ? { ...g, label: t('settings.bridgeSync.role.srOnlyGroup') } : g,
          )
        : GROUPS,
    [mirror, t],
  );
  const perms = toBig(draft.value.permissions);
  const values = useMemo(() => bitsToValues(perms), [perms]);
  const setPerm = (id: string, v: TriState) => {
    const bit = PERM[id]!;
    draft.set('permissions', String(v === 'allow' ? perms | bit : perms & ~bit));
  };
  const lockedReason = (id: string) =>
    locked
      ? t('settings.roles.readOnlyShort')
      : !viewer.canGrant(PERM[id]!)
        ? t('settings.roles.cannotChange')
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
            label: t('settings.roles.tabDisplay'),
            content: (
              <div className="ss-form">
                <TextInput
                  label={t('settings.roles.nameLabel')}
                  value={draft.value.name}
                  maxLength={100}
                  disabled={locked}
                  error={nameError}
                  onChange={(e) => draft.set('name', e.target.value)}
                />
                <div>
                  <RadioGroup
                    legend={t('settings.roles.styleLabel')}
                    value={colorStyleOf(draft.value)}
                    onValueChange={(v) => {
                      if (locked) return;
                      if (v === 'solid') {
                        draft.set('secondary', null);
                        draft.set('tertiary', null);
                      } else if (v === 'gradient') {
                        draft.set(
                          'secondary',
                          draft.value.secondary ?? draft.value.color ?? 0x5865f2,
                        );
                        draft.set('tertiary', null);
                      } else {
                        draft.set('color', HOLOGRAPHIC.primary);
                        draft.set('secondary', HOLOGRAPHIC.secondary);
                        draft.set('tertiary', HOLOGRAPHIC.tertiary);
                      }
                    }}
                  >
                    <Radio value="solid" label={t('settings.roles.styleSolid')} disabled={locked} />
                    <Radio
                      value="gradient"
                      label={t('settings.roles.styleGradient')}
                      disabled={locked}
                    />
                    <Radio
                      value="holographic"
                      label={t('settings.roles.styleHolographic')}
                      disabled={locked}
                    />
                  </RadioGroup>
                  {colorStyleOf(draft.value) !== 'holographic' && (
                    <>
                      <span className="ss-field-label">{t('settings.roles.colorLabel')}</span>
                      <ColorPicker
                        label={t('settings.roles.colorLabel')}
                        value={draft.value.color ? hexFromInt(draft.value.color) : '#99aab5'}
                        onValueChange={(hex) => !locked && draft.set('color', intFromHex(hex))}
                      />
                    </>
                  )}
                  {colorStyleOf(draft.value) === 'gradient' && (
                    <>
                      <span className="ss-field-label">{t('settings.roles.secondColorLabel')}</span>
                      <ColorPicker
                        label={t('settings.roles.secondColorLabel')}
                        value={hexFromInt(draft.value.secondary ?? 0)}
                        onValueChange={(hex) => !locked && draft.set('secondary', intFromHex(hex))}
                      />
                    </>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={
                      locked ||
                      (draft.value.color === 0 &&
                        draft.value.secondary === null &&
                        draft.value.tertiary === null)
                    }
                    onClick={() => {
                      draft.set('color', 0);
                      draft.set('secondary', null);
                      draft.set('tertiary', null);
                    }}
                  >
                    {t('settings.roles.defaultColor')}
                  </Button>
                  <p className="ss-sub">
                    {t('settings.roles.preview')}{' '}
                    <span
                      className="ss-role-preview"
                      data-gradient={draft.value.secondary != null ? '' : undefined}
                      data-testid="role-gradient-preview"
                      style={nameStyle(
                        stopsOf(
                          {
                            primary_color: draft.value.color,
                            secondary_color: draft.value.secondary,
                            tertiary_color: draft.value.tertiary,
                          },
                          draft.value.color,
                        ),
                      )}
                    >
                      {draft.value.name || role.name}
                    </span>
                  </p>
                </div>
                <ImageField
                  label={t('settings.roles.iconLabel')}
                  hint={t('settings.roles.iconHint')}
                  ownerId={guildId}
                  shape="square"
                  value={draft.value.icon}
                  disabled={locked}
                  onChange={(v) => {
                    draft.set('icon', v);
                    if (v && draft.value.unicode_emoji) draft.set('unicode_emoji', '');
                  }}
                />
                <TextInput
                  label={t('settings.roles.emojiLabel')}
                  hint={t('settings.roles.emojiHint')}
                  value={draft.value.unicode_emoji}
                  maxLength={16}
                  disabled={locked}
                  onChange={(e) => {
                    draft.set('unicode_emoji', e.target.value);
                    if (e.target.value && draft.value.icon) draft.set('icon', null);
                  }}
                />
                <Switch
                  label={t('settings.roles.hoist')}
                  checked={draft.value.hoist}
                  disabled={locked}
                  onChange={(e) => draft.set('hoist', e.target.checked)}
                />
                <Switch
                  label={t('settings.roles.mentionable')}
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
      label: isEveryone
        ? t('settings.roles.defaultPermissions')
        : t('settings.roles.tabPermissions'),
      content: (
        <div className="ss-form">
          {prefilled && (
            <Notice variant="info" title={t('settings.bridgeSync.conflicts.prefillTitle')}>
              {t('settings.bridgeSync.conflicts.prefillBody')}
            </Notice>
          )}
          {mirror && (
            <p>
              <Badge variant="accent">
                {mirror.state === 'frozen'
                  ? t('settings.bridgeSync.role.frozen')
                  : t('settings.bridgeSync.role.mirrored')}
              </Badge>{' '}
              <span className="ss-sub">{t('settings.bridgeSync.role.orderNote')}</span>
            </p>
          )}
          {has(perms, PERM.ADMINISTRATOR!) && (
            <Notice variant="warning" title={t('settings.roles.adminTitle')}>
              {t('settings.roles.adminBody')}
            </Notice>
          )}
          <PermissionMatrix
            groups={groups}
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
                {t('settings.roles.clearPermissions')}
              </Button>
            }
          />
          {mirror && mirror.discord_only.length > 0 && (
            <Notice variant="info" title={t('settings.bridgeSync.role.discordOnlyTitle')}>
              <p>{t('settings.bridgeSync.role.discordOnlyBody')}</p>
              <ul>
                {mirror.discord_only.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            </Notice>
          )}
          <StorageGrantsCard
            key={role.id}
            guildId={guildId}
            roleId={role.id}
            grants={role.sr_storage_grants ?? []}
            locked={locked || !viewer.can('MANAGE_ROLES')}
            onChanged={onChanged}
          />
        </div>
      ),
    },
    ...(!isEveryone
      ? [
          {
            id: 'members',
            label: t('settings.roles.tabMembers'),
            content: <RoleMembers guildId={guildId} viewer={viewer} role={role} />,
          },
          {
            id: 'links',
            label: t('settings.roles.tabLinks'),
            content: <NotAvailable>{t('settings.roles.linksNote')}</NotAvailable>,
          },
        ]
      : []),
  ];

  return (
    <section
      aria-label={
        isEveryone
          ? t('settings.roles.defaultPermissions')
          : t('settings.roles.editRole', { name: role.name })
      }
    >
      <h2 className="ss-title">{isEveryone ? t('settings.roles.defaultEveryone') : role.name}</h2>
      {reason && (
        <Notice variant="info" title={t('settings.roles.readOnlyTitle')} className="ss-status">
          {reason}
        </Notice>
      )}
      {role.managed && !isEveryone && (
        <Notice variant="info" className="ss-status">
          {t('settings.roles.managed')}
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      <Tabs
        items={tabs}
        label={t('settings.roles.settingsLabel')}
        className="ss-tabs"
        {...(prefilled ? { defaultValue: 'permissions' } : {})}
      />
      {!isEveryone && !role.managed && (
        <div className="ss-actions" style={{ marginTop: 24 }}>
          <Button
            variant="danger"
            disabled={!!reason}
            onClick={() => setConfirmDelete(true)}
            aria-label={t('settings.roles.deleteNamed', { name: role.name })}
          >
            {t('settings.roles.delete')}
          </Button>
        </div>
      )}
      <ConfirmDialog
        open={confirmDelete}
        danger
        title={t('settings.roles.deleteTitle', { name: role.name })}
        description={t('settings.roles.deleteDescription')}
        confirmLabel={t('settings.roles.delete')}
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
  const t = useT();
  const { members } = useMembers(guildId);
  const [query, setQuery] = useState('');
  const action = useAction();
  const reason =
    roleEditReason(viewer, role, guildId) ??
    (role.managed ? t('settings.roles.managedAssign') : undefined);
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
        aria-label={t('settings.roles.addMemberLabel')}
        placeholder={t('settings.roles.addMember')}
        disabled={!!reason}
        options={others.map((m) => ({ value: m.member.user_id, label: nameOf(m) }))}
        value={null}
        onValueChange={(id) => void assign(id)}
      />
      <TextInput
        label={t('settings.roles.searchMembers')}
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <ul className="ss-list" aria-label={t('settings.roles.membersWith', { name: role.name })}>
        {shown.map((m) => (
          <li key={m.member.user_id} className="ss-row">
            <span className="ss-grow ss-name">{nameOf(m)}</span>
            <Button
              size="sm"
              variant="secondary"
              disabled={!!reason}
              aria-label={t('settings.roles.removeFrom', { member: nameOf(m), role: role.name })}
              onClick={() => void unassign(m.member.user_id)}
            >
              {t('settings.roles.remove')}
            </Button>
          </li>
        ))}
      </ul>
      {shown.length === 0 && <p className="ss-na">{t('settings.roles.noHolders')}</p>}
    </div>
  );
}

/** The role's emoji or icon image beside its name (decorative: the name is next to it). */
function RoleBadge({ role, guildId }: { role: api.RoleObject; guildId: string }) {
  if (role.icon) {
    return (
      <img
        className="ss-role-icon"
        alt=""
        src={cdnUrl('icons', guildId, role.icon, { size: 32 }) ?? undefined}
      />
    );
  }
  return role.unicode_emoji ? (
    <span aria-hidden="true" className="ss-role-icon">
      {role.unicode_emoji}
    </span>
  ) : null;
}
