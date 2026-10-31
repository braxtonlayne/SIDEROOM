import { Button, Switch } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';
import * as api from './api';
import { useRolePrefill } from './mirrors';

type T = ReturnType<typeof useT>;

const KINDS = Object.keys(api.CONFLICT_ACTIONS) as api.ConflictKind[];

export function kindText(k: api.ConflictKind, t: T): string {
  switch (k) {
    case 'permission_reverted':
      return t('settings.bridgeSync.conflicts.kindPermissionReverted');
    case 'overwrite_trimmed':
      return t('settings.bridgeSync.conflicts.kindOverwriteTrimmed');
    case 'structure_conflict':
      return t('settings.bridgeSync.conflicts.kindStructureConflict');
    case 'frozen':
      return t('settings.bridgeSync.conflicts.kindFrozen');
    case 'held_deletion':
      return t('settings.bridgeSync.conflicts.kindHeldDeletion');
    case 'role_deletion_refused':
      return t('settings.bridgeSync.conflicts.kindRoleDeletionRefused');
    case 'dead_op':
      return t('settings.bridgeSync.conflicts.kindDeadOp');
    case 'cap':
      return t('settings.bridgeSync.conflicts.kindCap');
  }
}

function actionText(a: api.ConflictAction, t: T): string {
  switch (a) {
    case 'apply_on_sr':
      return t('settings.bridgeSync.conflicts.applyOnSr');
    case 'take_discord':
      return t('settings.bridgeSync.conflicts.takeDiscord');
    case 'restore':
      return t('settings.bridgeSync.conflicts.restore');
    case 'delete_now':
      return t('settings.bridgeSync.conflicts.deleteNow');
    case 'unfreeze':
      return t('settings.bridgeSync.conflicts.unfreeze');
    case 'retry':
      return t('settings.bridgeSync.conflicts.retry');
    case 'dismiss':
      return t('settings.bridgeSync.conflicts.dismiss');
  }
}

/**
 * The conflict log (BRG-SYN-004, design 15): what Discord did, who, what the bridge did, when and how often.
 * Every action runs as the clicking person through the ordinary SIDE ROOM checks; "Apply on SIDE ROOM" only opens
 * the role editor prefilled, and the person saves an ordinary edit.
 */
export function ConflictLog({ guildId }: { guildId: string }) {
  const t = useT();
  const navigate = useNavigate();
  const [kind, setKind] = useState<api.ConflictKind | ''>('');
  const [resolved, setResolved] = useState(false);
  const res = useResource(`bridge-conflicts-${guildId}-${kind}-${resolved}`, () =>
    api.listConflicts(guildId, {
      ...(kind ? { kind } : {}),
      ...(resolved ? {} : { open: true }),
    }),
  );
  const act = useAction();
  const setPrefill = useRolePrefill((s) => s.set);

  const run = (c: api.Conflict, a: api.ConflictAction) => {
    if (a === 'apply_on_sr') {
      if (c.object_kind === 'role' && c.object_id) {
        setPrefill({
          guildId,
          roleId: c.object_id,
          permissions: c.discord_permissions ?? [],
        });
        void navigate({
          to: '/settings/server/$guildId/$section',
          params: { guildId, section: 'roles' },
        });
      } else {
        void navigate({
          to: '/settings/server/$guildId/$section',
          params: { guildId, section: 'channels' },
        });
      }
      return;
    }
    void act.run(async () => {
      await api.runConflictAction(guildId, c.id, a);
      await res.reload();
    }, t('settings.bridgeSync.conflicts.done'));
  };

  return (
    <section
      className="ss-card"
      aria-labelledby="ss-bridge-conflicts"
      data-testid="bridge-conflicts"
    >
      <h2 id="ss-bridge-conflicts">{t('settings.bridgeSync.conflicts.title')}</h2>
      <div className="ss-toolbar">
        <label className="ss-field-label">
          {t('settings.bridgeSync.conflicts.filterKind')}
          <select
            className="ss-select"
            value={kind}
            onChange={(e) => setKind(e.target.value as api.ConflictKind | '')}
          >
            <option value="">{t('settings.bridgeSync.conflicts.allKinds')}</option>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {kindText(k, t)}
              </option>
            ))}
          </select>
        </label>
        <Switch
          label={t('settings.bridgeSync.conflicts.showResolved')}
          checked={resolved}
          onChange={(e) => setResolved(e.target.checked)}
        />
      </div>
      <Async res={res} label={t('settings.bridgeSync.conflicts.loading')}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="ss-na">{t('settings.bridgeSync.conflicts.empty')}</p>
          ) : (
            <ul
              className="ss-bridge-conflicts"
              aria-label={t('settings.bridgeSync.conflicts.list')}
            >
              {rows.map((c) => (
                <li key={c.id}>
                  <h3 className="ss-bridge-h">
                    {kindText(c.kind, t)}: {c.object_name}
                  </h3>
                  {c.discord_value && (
                    <p>
                      {t('settings.bridgeSync.conflicts.discordDid', { value: c.discord_value })}
                    </p>
                  )}
                  {c.sr_value && (
                    <p>{t('settings.bridgeSync.conflicts.srHas', { value: c.sr_value })}</p>
                  )}
                  <p>
                    {c.discord_actor
                      ? t('settings.bridgeSync.conflicts.who', { name: c.discord_actor })
                      : t('settings.bridgeSync.conflicts.whoUnknown')}
                  </p>
                  <p>{t('settings.bridgeSync.conflicts.bridgeDid', { value: c.bridge_did })}</p>
                  <p className="ss-sub">
                    {t('settings.bridgeSync.conflicts.when', {
                      when: formatDateTime(c.last_at * 1000),
                    })}
                    {', '}
                    {t('settings.bridgeSync.conflicts.times', { n: c.count })}
                    {c.resolved_at ? `, ${t('settings.bridgeSync.conflicts.resolved')}` : ''}
                  </p>
                  {!c.resolved_at && (
                    <div className="ss-actions">
                      {api.CONFLICT_ACTIONS[c.kind].map((a) => (
                        <Button
                          key={a}
                          size="sm"
                          variant={a === 'delete_now' ? 'danger' : 'secondary'}
                          disabled={act.busy}
                          aria-label={t('settings.bridgeSync.conflicts.actionLabel', {
                            action: actionText(a, t),
                            name: c.object_name,
                          })}
                          onClick={() => run(c, a)}
                        >
                          {actionText(a, t)}
                        </Button>
                      ))}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <ActionStatus error={act.error} done={act.done} />
    </section>
  );
}
