import { Button, EmptyState, Modal } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../../i18n';
import * as api from '../api';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';

/** Discord's two special targets: the `@everyone` role has the server's id, "All channels" is that id minus one. */
export const everyoneId = (guildId: string) => guildId;
export const allChannelsId = (guildId: string) => {
  try {
    return (BigInt(guildId) - 1n).toString();
  } catch {
    return guildId;
  }
};

type Entry = api.CommandPermissionEntry;
const ROLE = 1;
const MEMBER = 2;
const CHANNEL = 3;

/**
 * Command permissions for one installed application (J-10, LNG-SSET-047): who may use a command, set for the
 * application as a whole or per command, with Discord's rules. Roles and members are allowed or denied, channels
 * too; the "Everyone" and "All channels" rows are the fallbacks. Server managers (and the application's owner)
 * may write them; the overrides go to `PUT /applications/{app}/guilds/{guild}/commands/{command}/permissions`.
 */
export function BotCommandPermissions({
  guildId,
  appId,
  appName,
  onClose,
}: {
  guildId: string;
  appId: string;
  appName: string;
  onClose: () => void;
}) {
  const t = useT();
  const commands = useResource(`bot-commands:${guildId}:${appId}`, () =>
    api.listAppCommands(appId, guildId),
  );
  const saved = useResource(`bot-command-perms:${guildId}:${appId}`, () =>
    api.listCommandPermissions(appId, guildId),
  );
  const roles = useResource(`bot-perm-roles:${guildId}`, () => api.listRoles(guildId));
  const channels = useResource(`bot-perm-channels:${guildId}`, () => api.listChannels(guildId));
  const members = useResource(`bot-perm-members:${guildId}`, () =>
    api.listMembers(guildId, { limit: 100 }),
  );
  /** The target being edited: the application default (its own id) or one command. */
  const [target, setTarget] = useState(appId);
  const [draft, setDraft] = useState<Entry[]>([]);
  const [dirty, setDirty] = useState(false);
  const action = useAction();

  // Overrides saved for the target; replaced when the target changes or a save lands.
  const current = useMemo(
    () => saved.data?.find((p) => p.id === target)?.permissions ?? [],
    [saved.data, target],
  );
  useEffect(() => {
    setDraft(current);
    setDirty(false);
  }, [current]);

  const edit = (next: Entry[]) => {
    setDraft(next);
    setDirty(true);
  };
  const find = (id: string, type: number) => draft.find((e) => e.id === id && e.type === type);
  /** Set, flip or (with `undefined`) clear the override of one target. */
  const setEntry = (id: string, type: Entry['type'], permission: boolean | undefined) => {
    const rest = draft.filter((e) => !(e.id === id && e.type === type));
    edit(permission === undefined ? rest : [...rest, { id, type, permission }]);
  };

  const nameOf = (e: Entry): string => {
    if (e.type === ROLE) {
      if (e.id === everyoneId(guildId)) return t('settings.commandPermissions.everyone');
      return roles.data?.find((r) => r.id === e.id)?.name ?? e.id;
    }
    if (e.type === CHANNEL) {
      if (e.id === allChannelsId(guildId)) return t('settings.commandPermissions.allChannels');
      const c = channels.data?.find((c) => c.id === e.id);
      return c?.name ? `#${c.name}` : e.id;
    }
    const m = members.data?.find((m) => m.user.id === e.id);
    return m ? m.nick || api.userName(m.user) : e.id;
  };

  const special = [
    { id: everyoneId(guildId), type: ROLE as Entry['type'] },
    { id: allChannelsId(guildId), type: CHANNEL as Entry['type'] },
  ];
  const isSpecial = (e: Entry) => special.some((s) => s.id === e.id && s.type === e.type);
  const extra = draft.filter((e) => !isSpecial(e));

  // What can still be added.
  const addable = [
    ...(roles.data ?? [])
      .filter((r) => r.id !== guildId && !find(r.id, ROLE))
      .map((r) => ({ key: `1:${r.id}`, label: `@${r.name}` })),
    ...(members.data ?? [])
      .filter((m) => !find(m.user.id, MEMBER))
      .map((m) => ({ key: `2:${m.user.id}`, label: m.nick || api.userName(m.user) })),
    ...(channels.data ?? [])
      .filter((c) => c.type !== 4 && !find(c.id, CHANNEL))
      .map((c) => ({ key: `3:${c.id}`, label: `#${c.name ?? c.id}` })),
  ];

  const save = async () => {
    const out = await action.run(
      () => api.putCommandPermissions(appId, guildId, target, draft),
      t('settings.commandPermissions.saved'),
    );
    if (out) {
      saved.set((cur) => [...(cur ?? []).filter((p) => p.id !== target), out]);
      setDirty(false);
    }
  };

  const stateOf = (id: string, type: number) => {
    const e = find(id, type);
    return e === undefined ? 'default' : e.permission ? 'allow' : 'deny';
  };
  const fromState = (v: string): boolean | undefined =>
    v === 'allow' ? true : v === 'deny' ? false : undefined;

  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={t('settings.commandPermissions.title', { name: appName })}
      description={t('settings.commandPermissions.subtitle')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.commandPermissions.close')}
          </Button>
          <Button disabled={!dirty} loading={action.busy} onClick={() => void save()}>
            {t('settings.commandPermissions.save')}
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <Async res={commands} label={t('settings.commandPermissions.loading')}>
        {(list) => (
          <div className="ss-form" style={{ maxWidth: 'none' }}>
            <label className="ss-field-label" htmlFor="cmdperm-target">
              {t('settings.commandPermissions.command')}
            </label>
            <select
              id="cmdperm-target"
              className="ss-select"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
            >
              <option value={appId}>{t('settings.commandPermissions.allCommands')}</option>
              {list.map((c) => (
                <option key={c.id} value={c.id}>
                  /{c.name}
                </option>
              ))}
            </select>
            {list.length === 0 && (
              <EmptyState
                title={t('settings.commandPermissions.noCommands')}
                description={t('settings.commandPermissions.noCommandsHint')}
              />
            )}
            <ul className="ss-list" aria-label={t('settings.commandPermissions.rules')}>
              {special.map((s) => {
                const name = nameOf({ id: s.id, type: s.type, permission: true });
                return (
                  <li key={`${s.type}:${s.id}`} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">{name}</div>
                    </div>
                    <select
                      className="ss-select"
                      aria-label={t('settings.commandPermissions.ruleFor', { name })}
                      value={stateOf(s.id, s.type)}
                      onChange={(e) => setEntry(s.id, s.type, fromState(e.target.value))}
                    >
                      <option value="default">{t('settings.commandPermissions.default')}</option>
                      <option value="allow">{t('settings.commandPermissions.allow')}</option>
                      <option value="deny">{t('settings.commandPermissions.deny')}</option>
                    </select>
                  </li>
                );
              })}
              {extra.map((e) => {
                const name = nameOf(e);
                return (
                  <li key={`${e.type}:${e.id}`} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">{name}</div>
                      <div className="ss-meta">
                        {t(
                          e.type === ROLE
                            ? 'settings.commandPermissions.kindRole'
                            : e.type === MEMBER
                              ? 'settings.commandPermissions.kindMember'
                              : 'settings.commandPermissions.kindChannel',
                        )}
                      </div>
                    </div>
                    <select
                      className="ss-select"
                      aria-label={t('settings.commandPermissions.ruleFor', { name })}
                      value={e.permission ? 'allow' : 'deny'}
                      onChange={(ev) => setEntry(e.id, e.type, ev.target.value === 'allow')}
                    >
                      <option value="allow">{t('settings.commandPermissions.allow')}</option>
                      <option value="deny">{t('settings.commandPermissions.deny')}</option>
                    </select>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('settings.commandPermissions.removeNamed', { name })}
                      onClick={() => setEntry(e.id, e.type, undefined)}
                    >
                      {t('settings.commandPermissions.remove')}
                    </Button>
                  </li>
                );
              })}
            </ul>
            <label className="ss-field-label" htmlFor="cmdperm-add">
              {t('settings.commandPermissions.add')}
            </label>
            <select
              id="cmdperm-add"
              className="ss-select"
              value=""
              onChange={(e) => {
                const [type, id] = e.target.value.split(':');
                if (type && id) setEntry(id, Number(type) as Entry['type'], true);
              }}
            >
              <option value="" disabled>
                {t('settings.commandPermissions.addPlaceholder')}
              </option>
              {addable.map((a) => (
                <option key={a.key} value={a.key}>
                  {a.label}
                </option>
              ))}
            </select>
          </div>
        )}
      </Async>
    </Modal>
  );
}
