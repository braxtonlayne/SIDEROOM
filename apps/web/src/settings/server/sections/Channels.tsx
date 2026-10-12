import {
  Button,
  Combobox,
  ConfirmDialog,
  Notice,
  PermissionMatrix,
  Switch,
  Tabs,
  TextInput,
  type TriState,
} from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { CreateChannelDialog } from '../../../lounge/CreateChannelDialog';
import { useGuild, useMembers } from '../../../store';
import * as api from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm, type Viewer } from '../hooks';
import {
  PERM,
  PERMISSIONS,
  has,
  overwriteToValues,
  permissionGroups,
  toBig,
  valuesToOverwrite,
} from '../permissions';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

const GROUPS = permissionGroups();
const isCategory = (c: api.ChannelObject) => c.type === 4;
const isVoice = (c: api.ChannelObject) => c.type === 2 || c.type === 13;
const isText = (c: api.ChannelObject) => c.type === 0 || c.type === 5 || c.type === 15;
const bySort = (a: api.ChannelObject, b: api.ChannelObject) =>
  a.position - b.position || (BigInt(a.id) < BigInt(b.id) ? -1 : 1);

export const SLOWMODE = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];
const slowLabel = (s: number) =>
  s === 0 ? 'Off' : s < 60 ? `${s}s` : s < 3600 ? `${s / 60}m` : `${s / 3600}h`;

/** Permissions that make sense as overwrites on this channel type. */
export function overwriteApplies(id: string, channel: api.ChannelObject): boolean {
  const info = PERMISSIONS.find((p) => p.id === id);
  if (!info) return false;
  if (id === 'MANAGE_CHANNELS') return true;
  if (info.scope === 'guild') return false;
  if (info.scope === 'text') return !isVoice(channel);
  if (info.scope === 'voice') return !isText(channel);
  return true;
}

export function ChannelsSection({ guildId, viewer }: SectionProps) {
  const res = useResource(`channels:${guildId}`, () => api.listChannels(guildId));
  useGuildEvents(
    guildId,
    ['CHANNEL_CREATE', 'CHANNEL_UPDATE', 'CHANNEL_DELETE'],
    () => void res.reload(),
  );
  const [create, setCreate] = useState<null | { parentId?: string; category?: boolean }>(null);
  return (
    <>
      <SectionHeader
        title="Channels & Categories"
        description="Organize channels into categories and control who can see and use them."
        actions={
          <>
            <Button variant="secondary" onClick={() => setCreate({ category: true })}>
              Create Category
            </Button>
            <Button onClick={() => setCreate({})}>Create Channel</Button>
          </>
        }
      />
      <Async res={res} label="Loading channels">
        {(channels) => (
          <ChannelsEditor
            guildId={guildId}
            viewer={viewer}
            channels={channels}
            res={res}
            onCreate={setCreate}
          />
        )}
      </Async>
      {create && (
        <CreateChannelDialog
          guildId={guildId}
          parentId={create.parentId}
          category={create.category}
          onClose={() => {
            setCreate(null);
            void res.reload();
          }}
        />
      )}
    </>
  );
}

function ChannelsEditor({
  guildId,
  viewer,
  channels,
  res,
  onCreate,
}: {
  guildId: string;
  viewer: Viewer;
  channels: api.ChannelObject[];
  res: { reload: () => Promise<void> };
  onCreate: (c: { parentId?: string; category?: boolean }) => void;
}) {
  const action = useAction();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [blocked, setBlocked] = useState(false);

  const categories = channels.filter(isCategory).sort(bySort);
  const children = (id: string) =>
    channels.filter((c) => !isCategory(c) && c.parent_id === id).sort(bySort);
  const catIds = new Set(categories.map((c) => c.id));
  // Top level: categories and channels without a (known) category, in one ordering.
  const top = channels
    .filter((c) => isCategory(c) || !c.parent_id || !catIds.has(c.parent_id))
    .sort(bySort);
  const selected = channels.find((c) => c.id === selectedId);

  const select = (id: string) => {
    if (dirty && id !== selectedId) {
      setBlocked(true);
      return;
    }
    setBlocked(false);
    setSelectedId(id);
  };

  /** Swap `id` with its neighbour among `siblings`, renumbering the whole sibling set contiguously. */
  const move = async (siblings: api.ChannelObject[], id: string, delta: -1 | 1) => {
    const from = siblings.findIndex((c) => c.id === id);
    const to = from + delta;
    if (from < 0 || to < 0 || to >= siblings.length) return;
    const next = [...siblings];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item!);
    const moves = next.map((c, i) => ({ id: c.id, position: i }));
    await action.run(() => api.reorderChannels(guildId, moves));
    await res.reload();
  };

  const row = (c: api.ChannelObject, siblings: api.ChannelObject[], nested: boolean) => {
    const i = siblings.findIndex((x) => x.id === c.id);
    const label = c.name ?? c.id;
    return (
      <li key={c.id} className={`ss-row${nested ? ' ss-tree-child' : ''}`}>
        <button
          type="button"
          className={`ss-grow ss-name ss-link${isCategory(c) ? ' ss-tree-cat' : ''}`}
          aria-current={selectedId === c.id ? 'true' : undefined}
          onClick={() => select(c.id)}
        >
          {isCategory(c) ? label : `${isVoice(c) ? '\u{1F50A} ' : '# '}${label}`}
        </button>
        {isCategory(c) && (
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Create channel in ${label}`}
            onClick={() => onCreate({ parentId: c.id })}
          >
            Add channel
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={`Move ${label} up`}
          disabled={i <= 0}
          onClick={() => void move(siblings, c.id, -1)}
        >
          <span aria-hidden="true">{'↑'}</span>
        </Button>
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={`Move ${label} down`}
          disabled={i < 0 || i >= siblings.length - 1}
          onClick={() => void move(siblings, c.id, 1)}
        >
          <span aria-hidden="true">{'↓'}</span>
        </Button>
      </li>
    );
  };

  return (
    <div className="ss-split">
      <div>
        <ActionStatus error={action.error} />
        {blocked && (
          <Notice variant="warning" onDismiss={() => setBlocked(false)}>
            Save or reset your changes before selecting another channel.
          </Notice>
        )}
        <ul className="ss-list" aria-label="Channels and categories">
          {top.map((c) =>
            isCategory(c) ? (
              <li key={c.id} className="ss-tree-group">
                <ul className="ss-list" aria-label={c.name ?? 'Category'}>
                  {row(c, top, false)}
                  {(() => {
                    const kids = children(c.id);
                    return kids.map((k) => row(k, kids, true));
                  })()}
                </ul>
              </li>
            ) : (
              row(c, top, false)
            ),
          )}
        </ul>
        {top.length === 0 && <p className="ss-na">This server has no channels yet.</p>}
      </div>
      {selected ? (
        <ChannelEditor
          key={selected.id}
          guildId={guildId}
          viewer={viewer}
          channel={selected}
          channels={channels}
          categories={categories}
          onDirty={setDirty}
          onChanged={res.reload}
          onDeleted={() => {
            setSelectedId(null);
            setDirty(false);
          }}
        />
      ) : (
        <p className="ss-na">Select a channel or category to edit it.</p>
      )}
    </div>
  );
}

interface ChannelFields {
  name: string;
  topic: string;
  nsfw: boolean;
  rate_limit_per_user: number;
  user_limit: number;
  parent_id: string;
}

function ChannelEditor({
  guildId,
  viewer,
  channel,
  channels,
  categories,
  onDirty,
  onChanged,
  onDeleted,
}: {
  guildId: string;
  viewer: Viewer;
  channel: api.ChannelObject;
  channels: api.ChannelObject[];
  categories: api.ChannelObject[];
  onDirty: (d: boolean) => void;
  onChanged: () => Promise<void>;
  onDeleted: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const action = useAction();
  const cat = isCategory(channel);

  const remove = async () => {
    const ok = await action.run(async () => {
      await api.deleteChannel(channel.id);
      return true;
    });
    if (ok) {
      setConfirmDelete(false);
      onDeleted();
      await onChanged();
    }
  };

  const tabs = [
    {
      id: 'overview',
      label: 'Overview',
      content: (
        <ChannelOverview
          guildId={guildId}
          channel={channel}
          categories={categories}
          onDirty={onDirty}
          onChanged={onChanged}
        />
      ),
    },
    {
      id: 'permissions',
      label: 'Permissions',
      content: (
        <ChannelPermissions
          guildId={guildId}
          viewer={viewer}
          channel={channel}
          channels={channels}
          onDirty={onDirty}
          onChanged={onChanged}
        />
      ),
    },
  ];
  return (
    <section aria-label={`Edit ${cat ? 'category' : 'channel'} ${channel.name ?? ''}`}>
      <h2 className="ss-title">{cat ? channel.name : `# ${channel.name}`}</h2>
      <ActionStatus error={action.error} />
      <Tabs items={tabs} label="Channel settings" className="ss-tabs" />
      <div className="ss-actions" style={{ marginTop: 24 }}>
        <Button variant="danger" onClick={() => setConfirmDelete(true)}>
          {cat ? 'Delete category' : 'Delete channel'}
        </Button>
      </div>
      <ConfirmDialog
        open={confirmDelete}
        danger
        title={`Delete ${channel.name ?? 'channel'}`}
        description={
          cat
            ? 'Channels in this category are kept and move to the top level.'
            : 'Messages in this channel are removed. This cannot be undone.'
        }
        confirmLabel={cat ? 'Delete category' : 'Delete channel'}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setConfirmDelete(false)}
      />
    </section>
  );
}

function ChannelOverview({
  channel,
  categories,
  onDirty,
  onChanged,
}: {
  guildId: string;
  channel: api.ChannelObject;
  categories: api.ChannelObject[];
  onDirty: (d: boolean) => void;
  onChanged: () => Promise<void>;
}) {
  const base = useMemo<ChannelFields>(
    () => ({
      name: channel.name ?? '',
      topic: channel.topic ?? '',
      nsfw: channel.nsfw,
      rate_limit_per_user: channel.rate_limit_per_user,
      user_limit: channel.user_limit ?? 0,
      parent_id: channel.parent_id ?? '',
    }),
    [channel],
  );
  const draft = useDraft(base);
  const action = useAction();
  const cat = isCategory(channel);
  const name = draft.value.name.trim();
  const nameError =
    draft.changed.name !== undefined && (name.length < 1 || name.length > 100)
      ? 'Name must be between 1 and 100 characters.'
      : undefined;

  const save = useCallback(async () => {
    if (nameError) return;
    const c = draft.changed;
    const body: api.ChannelBody = {};
    if (c.name !== undefined) body.name = name;
    if (c.topic !== undefined) body.topic = c.topic.trim() || null;
    if (c.nsfw !== undefined) body.nsfw = c.nsfw;
    if (c.rate_limit_per_user !== undefined) body.rate_limit_per_user = c.rate_limit_per_user;
    if (c.parent_id !== undefined) body.parent_id = c.parent_id || null;
    const ok = await action.run(() => api.modifyChannel(channel.id, body), 'Channel saved.');
    if (ok) {
      draft.reset();
      await onChanged();
    }
  }, [nameError, draft, name, action, channel.id, onChanged]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });
  useEffect(() => onDirty(draft.dirty), [onDirty, draft.dirty]);

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <TextInput
        label={cat ? 'Category name' : 'Channel name'}
        value={draft.value.name}
        maxLength={100}
        error={nameError}
        onChange={(e) => draft.set('name', e.target.value)}
      />
      {isText(channel) && (
        <>
          <TextInput
            label="Channel topic"
            value={draft.value.topic}
            maxLength={1024}
            onChange={(e) => draft.set('topic', e.target.value)}
          />
          <div>
            <label className="ss-field-label" htmlFor="ss-slowmode">
              Slowmode
            </label>
            <select
              id="ss-slowmode"
              className="ss-select"
              value={draft.value.rate_limit_per_user}
              onChange={(e) => draft.set('rate_limit_per_user', Number(e.target.value))}
            >
              {SLOWMODE.map((s) => (
                <option key={s} value={s}>
                  {slowLabel(s)}
                </option>
              ))}
            </select>
          </div>
          <Switch
            label="Age-restricted channel"
            checked={draft.value.nsfw}
            onChange={(e) => draft.set('nsfw', e.target.checked)}
          />
        </>
      )}
      {!cat && (
        <div>
          <label className="ss-field-label" htmlFor="ss-parent">
            Category
          </label>
          <select
            id="ss-parent"
            className="ss-select"
            value={draft.value.parent_id}
            onChange={(e) => draft.set('parent_id', e.target.value)}
          >
            <option value="">No category</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}

interface Target {
  id: string;
  type: 0 | 1;
}
const tkey = (t: Target) => `${t.type}:${t.id}`;

function ChannelPermissions({
  guildId,
  viewer,
  channel,
  channels,
  onDirty,
  onChanged,
}: {
  guildId: string;
  viewer: Viewer;
  channel: api.ChannelObject;
  channels: api.ChannelObject[];
  onDirty: (d: boolean) => void;
  onChanged: () => Promise<void>;
}) {
  const { roles } = useGuild(guildId);
  const { members } = useMembers(guildId);
  const action = useAction();
  const [pending, setPending] = useState<Target[]>([]);
  const [targetKey, setTargetKey] = useState<string | null>(null);

  const overwrites = channel.permission_overwrites;
  const parent = channel.parent_id ? channels.find((c) => c.id === channel.parent_id) : undefined;
  const synced = !!parent && sameOverwrites(overwrites, parent.permission_overwrites);

  const nameOf = (t: Target) => {
    if (t.type === 0) return roles.find((r) => r.id === t.id)?.name ?? `Role ${t.id}`;
    const m = members.find((x) => x.member.user_id === t.id);
    return m ? m.member.nick || m.user?.global_name || m.user?.username || t.id : `Member ${t.id}`;
  };
  const targets: Target[] = [
    ...overwrites.map((o) => ({ id: o.id, type: (o.type === 1 ? 1 : 0) as 0 | 1 })),
    ...pending.filter((p) => !overwrites.some((o) => o.id === p.id && o.type === p.type)),
  ];
  const current = targets.find((t) => tkey(t) === targetKey) ?? targets[0];

  const needsManageRoles = !viewer.can('MANAGE_ROLES');
  const sync = async () => {
    const ok = await action.run(async () => {
      await api.modifyChannel(channel.id, { lock_permissions: true });
      return true;
    }, 'Permissions synced with the category.');
    if (ok) await onChanged();
  };

  const roleOptions = roles
    .filter((r) => !targets.some((t) => t.type === 0 && t.id === r.id))
    .map((r) => ({ value: r.id, label: r.name }));
  const memberOptions = members
    .filter((m) => !targets.some((t) => t.type === 1 && t.id === m.member.user_id))
    .map((m) => ({
      value: m.member.user_id,
      label: m.member.nick || m.user?.global_name || m.user?.username || m.member.user_id,
    }));
  const add = (t: Target) => {
    setPending((p) => [...p, t]);
    setTargetKey(tkey(t));
  };

  return (
    <div className="ss-form">
      {needsManageRoles && (
        <Notice variant="info" title="Read only">
          You need the Manage Roles permission to change who can use this channel.
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      {parent && (
        <Switch
          label="Sync permissions with category"
          description={
            synced
              ? `Synced with ${parent.name}. Editing any overwrite below un-syncs this channel.`
              : `Turn on to replace this channel's overwrites with those of ${parent.name}.`
          }
          checked={synced}
          disabled={needsManageRoles || synced || action.busy}
          onChange={() => void sync()}
        />
      )}
      <div className="ss-toolbar">
        <Combobox
          aria-label="Add a role"
          placeholder="Add a role"
          disabled={needsManageRoles}
          options={roleOptions}
          value={null}
          onValueChange={(id) => add({ id, type: 0 })}
        />
        <Combobox
          aria-label="Add a member"
          placeholder="Add a member"
          disabled={needsManageRoles}
          options={memberOptions}
          value={null}
          onValueChange={(id) => add({ id, type: 1 })}
        />
      </div>
      {targets.length === 0 ? (
        <p className="ss-na">No overwrites. Everyone uses the server role permissions here.</p>
      ) : (
        <>
          <ul className="ss-chips" aria-label="Roles and members with overwrites">
            {targets.map((t) => (
              <li key={tkey(t)} style={{ listStyle: 'none' }}>
                <button
                  type="button"
                  className="ss-role-chip ss-link"
                  aria-pressed={current !== undefined && tkey(current) === tkey(t)}
                  onClick={() => setTargetKey(tkey(t))}
                >
                  {t.type === 1 ? '@' : ''}
                  {nameOf(t)}
                </button>
              </li>
            ))}
          </ul>
          {current && (
            <OverwriteEditor
              key={tkey(current)}
              channel={channel}
              viewer={viewer}
              target={current}
              name={nameOf(current)}
              existing={overwrites.find(
                (o) => o.id === current.id && (o.type === 1 ? 1 : 0) === current.type,
              )}
              roles={roles}
              disabled={needsManageRoles}
              onDirty={onDirty}
              onSaved={async () => {
                setPending((p) => p.filter((x) => tkey(x) !== tkey(current)));
                await onChanged();
              }}
              onRemoved={async () => {
                setPending((p) => p.filter((x) => tkey(x) !== tkey(current)));
                setTargetKey(null);
                await onChanged();
              }}
            />
          )}
        </>
      )}
    </div>
  );
}

function sameOverwrites(a: api.OverwriteObject[], b: api.OverwriteObject[]) {
  return (
    a.length === b.length &&
    a.every((o) =>
      b.some((p) => p.id === o.id && p.type === o.type && p.allow === o.allow && p.deny === o.deny),
    )
  );
}

function OverwriteEditor({
  channel,
  viewer,
  target,
  name,
  existing,
  roles,
  disabled,
  onDirty,
  onSaved,
  onRemoved,
}: {
  channel: api.ChannelObject;
  viewer: Viewer;
  target: Target;
  name: string;
  existing: api.OverwriteObject | undefined;
  roles: { id: string; position: number }[];
  disabled: boolean;
  onDirty: (d: boolean) => void;
  onSaved: () => Promise<void>;
  onRemoved: () => Promise<void>;
}) {
  const base = useMemo(
    () => ({
      values: overwriteToValues(toBig(existing?.allow), toBig(existing?.deny)) as Record<
        string,
        TriState
      >,
    }),
    [existing],
  );
  const draft = useDraft(base);
  const action = useAction();
  const role = target.type === 0 ? roles.find((r) => r.id === target.id) : undefined;
  const hierarchyBlock =
    !!role &&
    role.id !== channel.guild_id &&
    !viewer.isOwner &&
    viewer.topPosition <= role.position;
  const locked = disabled || hierarchyBlock;

  const save = useCallback(async () => {
    const { allow, deny } = valuesToOverwrite(draft.value.values);
    const empty = allow === 0n && deny === 0n;
    const ok = await action.run(async () => {
      if (empty && existing) await api.deleteOverwrite(channel.id, target.id);
      else if (!empty)
        await api.putOverwrite(channel.id, target.id, {
          type: target.type,
          allow: allow.toString(),
          deny: deny.toString(),
        });
      return true;
    }, 'Permissions saved.');
    if (ok) {
      draft.reset();
      await onSaved();
    }
  }, [draft, action, existing, channel.id, target, onSaved]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });
  useEffect(() => onDirty(draft.dirty), [onDirty, draft.dirty]);

  const remove = async () => {
    const ok = await action.run(async () => {
      if (existing) await api.deleteOverwrite(channel.id, target.id);
      return true;
    });
    if (ok) await onRemoved();
  };

  const lockedReason = (id: string) => {
    if (hierarchyBlock) return undefined;
    const bit = PERM[id]!;
    if (!viewer.canGrant(bit) && !has(viewer.perms, PERM.ADMINISTRATOR!))
      return 'You cannot change a permission you do not have.';
    return undefined;
  };

  return (
    <div>
      <h3 className="ss-title" style={{ fontSize: '1rem' }}>
        {target.type === 1 ? 'Member' : 'Role'}: {name}
      </h3>
      {hierarchyBlock && (
        <Notice variant="info" className="ss-status">
          You cannot edit overwrites for a role at or above your highest role.
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      <PermissionMatrix
        groups={GROUPS}
        values={draft.value.values}
        mode="tristate"
        hidden={(id) => !overwriteApplies(id, channel)}
        lockedReason={(id) => (locked ? 'Read only.' : lockedReason(id))}
        onChange={(id, v) => {
          if (locked) return;
          const next = { ...draft.value.values };
          if (v === 'neutral') delete next[id];
          else next[id] = v;
          draft.set('values', next);
        }}
        toolbar={
          <Button size="sm" variant="secondary" disabled={locked} onClick={() => void remove()}>
            Remove overwrite
          </Button>
        }
      />
    </div>
  );
}
