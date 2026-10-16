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
import { t as translate, useT } from '../../../i18n';
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
import { ChannelFeatures, ChannelIntegrations, ChannelInvites } from './ChannelTabs';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

const GROUPS = permissionGroups();
const isCategory = (c: api.ChannelObject) => c.type === 4;
const isVoice = (c: api.ChannelObject) => c.type === 2 || c.type === 13;
const isText = (c: api.ChannelObject) => c.type === 0 || c.type === 5 || c.type === 15;
const bySort = (a: api.ChannelObject, b: api.ChannelObject) =>
  a.position - b.position || (BigInt(a.id) < BigInt(b.id) ? -1 : 1);

export const SLOWMODE = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];
const slowLabel = (s: number) =>
  s === 0
    ? translate('settings.channelTabs.slowOff')
    : s < 60
      ? `${s}s`
      : s < 3600
        ? `${s / 60}m`
        : `${s / 3600}h`;

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
  const t = useT();
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
        title={t('settings.channels.title')}
        description={t('settings.channels.description')}
        actions={
          <>
            <Button variant="secondary" onClick={() => setCreate({ category: true })}>
              {t('settings.channels.createCategory')}
            </Button>
            <Button onClick={() => setCreate({})}>{t('settings.channels.createChannel')}</Button>
          </>
        }
      />
      <Async res={res} label={t('settings.channels.loading')}>
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
  const t = useT();
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
            aria-label={t('settings.channels.createIn', { name: label })}
            onClick={() => onCreate({ parentId: c.id })}
          >
            {t('settings.channels.addChannel')}
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={t('settings.channels.moveUp', { name: label })}
          disabled={i <= 0}
          onClick={() => void move(siblings, c.id, -1)}
        >
          <span aria-hidden="true">{'↑'}</span>
        </Button>
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={t('settings.channels.moveDown', { name: label })}
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
            {t('settings.channels.saveFirst')}
          </Notice>
        )}
        <ul className="ss-list" aria-label={t('settings.channels.listLabel')}>
          {top.map((c) =>
            isCategory(c) ? (
              <li key={c.id} className="ss-tree-group">
                <ul className="ss-list" aria-label={c.name ?? t('settings.channels.category')}>
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
        {top.length === 0 && <p className="ss-na">{t('settings.channels.none')}</p>}
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
        <p className="ss-na">{t('settings.channels.select')}</p>
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
  /** Voice bitrate in kbit/s. */
  bitrate_kbps: number;
  /** Video quality "full" (`video_quality_mode` 2) instead of auto (1). */
  video_full: boolean;
  /** Text channel is an announcement channel (type 5). */
  announcement: boolean;
  parent_id: string;
}

export function ChannelEditor({
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
  const t = useT();
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
      label: t('settings.channels.tabOverview'),
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
      label: t('settings.channels.tabPermissions'),
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
    ...(cat
      ? []
      : [
          {
            id: 'invites',
            label: t('settings.channels.tabInvites'),
            content: <ChannelInvites channel={channel} />,
          },
          {
            id: 'integrations',
            label: t('settings.channels.tabIntegrations'),
            content: <ChannelIntegrations guildId={guildId} channel={channel} />,
          },
          {
            id: 'features',
            label: t('settings.channels.tabFeatures'),
            content: <ChannelFeatures channel={channel} onChanged={onChanged} />,
          },
        ]),
  ];
  return (
    <section
      aria-label={
        cat
          ? t('settings.channels.editCategory', { name: channel.name ?? '' })
          : t('settings.channels.editChannel', { name: channel.name ?? '' })
      }
    >
      <h2 className="ss-title">{cat ? channel.name : `# ${channel.name}`}</h2>
      <ActionStatus error={action.error} />
      <Tabs items={tabs} label={t('settings.channels.settingsLabel')} className="ss-tabs" />
      <div className="ss-actions" style={{ marginTop: 24 }}>
        <Button variant="danger" onClick={() => setConfirmDelete(true)}>
          {cat ? t('settings.channels.deleteCategory') : t('settings.channels.deleteChannel')}
        </Button>
      </div>
      <ConfirmDialog
        open={confirmDelete}
        danger
        title={t('settings.channels.deleteTitle', {
          name: channel.name ?? t('settings.channels.channelWord'),
        })}
        description={
          cat
            ? t('settings.channels.deleteCategoryDescription')
            : t('settings.channels.deleteChannelDescription')
        }
        confirmLabel={
          cat ? t('settings.channels.deleteCategory') : t('settings.channels.deleteChannel')
        }
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
  const t = useT();
  const base = useMemo<ChannelFields>(
    () => ({
      name: channel.name ?? '',
      topic: channel.topic ?? '',
      nsfw: channel.nsfw,
      rate_limit_per_user: channel.rate_limit_per_user,
      user_limit: channel.user_limit ?? 0,
      bitrate_kbps: Math.round((channel.bitrate ?? 64_000) / 1000),
      video_full: channel.video_quality_mode === 2,
      announcement: channel.type === 5,
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
      ? t('settings.channels.nameError')
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
    if (c.user_limit !== undefined) body.user_limit = c.user_limit;
    if (c.bitrate_kbps !== undefined) body.bitrate = c.bitrate_kbps * 1000;
    if (c.video_full !== undefined) body.video_quality_mode = c.video_full ? 2 : 1;
    if (c.announcement !== undefined) body.type = c.announcement ? 5 : 0;
    const ok = await action.run(
      () => api.modifyChannel(channel.id, body),
      t('settings.channels.saved'),
    );
    if (ok) {
      draft.reset();
      await onChanged();
    }
  }, [nameError, draft, name, action, channel.id, onChanged, t]);

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
        label={cat ? t('settings.channels.categoryName') : t('settings.channels.channelName')}
        value={draft.value.name}
        maxLength={100}
        error={nameError}
        onChange={(e) => draft.set('name', e.target.value)}
      />
      {isText(channel) && (
        <>
          <TextInput
            label={t('settings.channels.topic')}
            value={draft.value.topic}
            maxLength={1024}
            onChange={(e) => draft.set('topic', e.target.value)}
          />
          <div>
            <label className="ss-field-label" htmlFor="ss-slowmode">
              {t('settings.channels.slowmode')}
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
            label={t('settings.channels.ageRestricted')}
            checked={draft.value.nsfw}
            onChange={(e) => draft.set('nsfw', e.target.checked)}
          />
          {(channel.type === 0 || channel.type === 5) && (
            <Switch
              label={t('settings.channels.announcement')}
              description={t('settings.channels.announcementHint')}
              checked={draft.value.announcement}
              onChange={(e) => draft.set('announcement', e.target.checked)}
            />
          )}
        </>
      )}
      {isVoice(channel) && (
        <>
          <div>
            <label className="ss-field-label" htmlFor="ss-user-limit">
              {t('settings.channels.userLimit', { count: draft.value.user_limit })}
            </label>
            <input
              id="ss-user-limit"
              type="range"
              min={0}
              max={99}
              value={draft.value.user_limit}
              onChange={(e) => draft.set('user_limit', Number(e.target.value))}
            />
          </div>
          <div>
            <label className="ss-field-label" htmlFor="ss-bitrate">
              {t('settings.channels.bitrate', { kbps: draft.value.bitrate_kbps })}
            </label>
            <input
              id="ss-bitrate"
              type="range"
              min={8}
              max={384}
              step={8}
              value={draft.value.bitrate_kbps}
              onChange={(e) => draft.set('bitrate_kbps', Number(e.target.value))}
            />
          </div>
          <Switch
            label={t('settings.channels.videoQuality')}
            description={t('settings.channels.videoQualityHint')}
            checked={draft.value.video_full}
            onChange={(e) => draft.set('video_full', e.target.checked)}
          />
          <p className="ss-sub">{t('settings.channels.region')}</p>
        </>
      )}
      {!cat && (
        <div>
          <label className="ss-field-label" htmlFor="ss-parent">
            {t('settings.channels.categoryLabel')}
          </label>
          <select
            id="ss-parent"
            className="ss-select"
            value={draft.value.parent_id}
            onChange={(e) => draft.set('parent_id', e.target.value)}
          >
            <option value="">{t('settings.channels.noCategory')}</option>
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
  const t = useT();
  const { roles } = useGuild(guildId);
  const { members } = useMembers(guildId);
  const action = useAction();
  const [pending, setPending] = useState<Target[]>([]);
  const [targetKey, setTargetKey] = useState<string | null>(null);

  const overwrites = channel.permission_overwrites;
  const parent = channel.parent_id ? channels.find((c) => c.id === channel.parent_id) : undefined;
  const synced = !!parent && sameOverwrites(overwrites, parent.permission_overwrites);

  const nameOf = (tg: Target) => {
    if (tg.type === 0)
      return (
        roles.find((r) => r.id === tg.id)?.name ??
        t('settings.channels.roleFallback', { id: tg.id })
      );
    const m = members.find((x) => x.member.user_id === tg.id);
    return m
      ? m.member.nick || m.user?.global_name || m.user?.username || tg.id
      : t('settings.channels.memberFallback', { id: tg.id });
  };
  const targets: Target[] = [
    ...overwrites.map((o) => ({ id: o.id, type: (o.type === 1 ? 1 : 0) as 0 | 1 })),
    ...pending.filter((p) => !overwrites.some((o) => o.id === p.id && o.type === p.type)),
  ];
  const current = targets.find((tg) => tkey(tg) === targetKey) ?? targets[0];

  const needsManageRoles = !viewer.can('MANAGE_ROLES');
  const sync = async () => {
    const ok = await action.run(async () => {
      await api.modifyChannel(channel.id, { lock_permissions: true });
      return true;
    }, t('settings.channels.synced'));
    if (ok) await onChanged();
  };

  const roleOptions = roles
    .filter((r) => !targets.some((tg) => tg.type === 0 && tg.id === r.id))
    .map((r) => ({ value: r.id, label: r.name }));
  const memberOptions = members
    .filter((m) => !targets.some((tg) => tg.type === 1 && tg.id === m.member.user_id))
    .map((m) => ({
      value: m.member.user_id,
      label: m.member.nick || m.user?.global_name || m.user?.username || m.member.user_id,
    }));
  const add = (tg: Target) => {
    setPending((p) => [...p, tg]);
    setTargetKey(tkey(tg));
  };

  return (
    <div className="ss-form">
      {needsManageRoles && (
        <Notice variant="info" title={t('settings.channels.readOnlyTitle')}>
          {t('settings.channels.readOnlyBody')}
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      {parent && (
        <Switch
          label={t('settings.channels.syncLabel')}
          description={
            synced
              ? t('settings.channels.syncedWith', { name: parent.name ?? '' })
              : t('settings.channels.syncTurnOn', { name: parent.name ?? '' })
          }
          checked={synced}
          disabled={needsManageRoles || synced || action.busy}
          onChange={() => void sync()}
        />
      )}
      <div className="ss-toolbar">
        <Combobox
          aria-label={t('settings.channels.addRole')}
          placeholder={t('settings.channels.addRole')}
          disabled={needsManageRoles}
          options={roleOptions}
          value={null}
          onValueChange={(id) => add({ id, type: 0 })}
        />
        <Combobox
          aria-label={t('settings.channels.addMember')}
          placeholder={t('settings.channels.addMember')}
          disabled={needsManageRoles}
          options={memberOptions}
          value={null}
          onValueChange={(id) => add({ id, type: 1 })}
        />
      </div>
      {targets.length === 0 ? (
        <p className="ss-na">{t('settings.channels.noOverwrites')}</p>
      ) : (
        <>
          <ul className="ss-chips" aria-label={t('settings.channels.overwritesLabel')}>
            {targets.map((tg) => (
              <li key={tkey(tg)} style={{ listStyle: 'none' }}>
                <button
                  type="button"
                  className="ss-role-chip ss-link"
                  aria-pressed={current !== undefined && tkey(current) === tkey(tg)}
                  onClick={() => setTargetKey(tkey(tg))}
                >
                  {tg.type === 1 ? '@' : ''}
                  {nameOf(tg)}
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
  const t = useT();
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
    }, t('settings.channels.permsSaved'));
    if (ok) {
      draft.reset();
      await onSaved();
    }
  }, [draft, action, existing, channel.id, target, onSaved, t]);

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
      return t('settings.channels.cannotChange');
    return undefined;
  };

  return (
    <div>
      <h3 className="ss-title" style={{ fontSize: '1rem' }}>
        {target.type === 1
          ? t('settings.channels.memberHeading', { name })
          : t('settings.channels.roleHeading', { name })}
      </h3>
      {hierarchyBlock && (
        <Notice variant="info" className="ss-status">
          {t('settings.channels.hierarchy')}
        </Notice>
      )}
      <ActionStatus error={action.error} done={action.done} />
      <PermissionMatrix
        groups={GROUPS}
        values={draft.value.values}
        mode="tristate"
        hidden={(id) => !overwriteApplies(id, channel)}
        lockedReason={(id) => (locked ? t('settings.channels.readOnlyShort') : lockedReason(id))}
        onChange={(id, v) => {
          if (locked) return;
          const next = { ...draft.value.values };
          if (v === 'neutral') delete next[id];
          else next[id] = v;
          draft.set('values', next);
        }}
        toolbar={
          <Button size="sm" variant="secondary" disabled={locked} onClick={() => void remove()}>
            {t('settings.channels.removeOverwrite')}
          </Button>
        }
      />
    </div>
  );
}
