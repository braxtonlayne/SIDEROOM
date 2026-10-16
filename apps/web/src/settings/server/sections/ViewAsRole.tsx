// View Server As Role (J-09, LNG-SSET-036): pick roles and see the channel list a member holding only those roles
// would get. Nothing is saved; the visibility is computed in the browser with the shared permissions engine.
import { Button, Checkbox, Modal, Notice } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../../i18n';
import { loadPermEngine, usePermEngine } from '../../../lounge/permEngine';
import * as api from '../api';
import { hasAdministrator, viewServerAs } from '../viewAs';

const ICON: Record<number, string> = {
  0: '#',
  2: '\u{1F50A}',
  4: '',
  5: '\u{1F4E3}',
  13: '\u{1F399}',
};

export function ViewAsRoleDialog({
  guildId,
  roles,
  onClose,
}: {
  guildId: string;
  roles: readonly api.RoleObject[];
  onClose: () => void;
}) {
  const t = useT();
  const engine = usePermEngine();
  const [loadFailed, setLoadFailed] = useState(false);
  const [channels, setChannels] = useState<api.ChannelObject[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    let live = true;
    loadPermEngine().catch(() => live && setLoadFailed(true));
    api.listChannels(guildId).then(
      (c) => live && setChannels(c),
      (e: unknown) => live && setError(api.errorText(e)),
    );
    return () => {
      live = false;
    };
  }, [guildId]);

  const pickable = useMemo(
    () => roles.filter((r) => r.id !== guildId).sort((a, b) => b.position - a.position),
    [roles, guildId],
  );
  const entries = useMemo(
    () =>
      engine && channels ? viewServerAs(engine, { guildId, roles, picked, channels }) : undefined,
    [engine, channels, guildId, roles, picked],
  );
  const admin = hasAdministrator(roles, picked);
  const toggle = (id: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const shown = entries?.filter((e) => e.visible) ?? [];

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('settings.viewAs.title')}
      description={t('settings.viewAs.description')}
      footer={
        <Button variant="secondary" onClick={onClose}>
          {t('settings.viewAs.close')}
        </Button>
      }
    >
      <div className="ss-form">
        <fieldset className="ss-fieldset">
          <legend className="ss-field-label">{t('settings.viewAs.rolesLegend')}</legend>
          {pickable.length === 0 && <p className="ss-na">{t('settings.viewAs.noRoles')}</p>}
          {pickable.map((r) => (
            <Checkbox
              key={r.id}
              label={r.name}
              checked={picked.has(r.id)}
              onChange={() => toggle(r.id)}
            />
          ))}
        </fieldset>
        {(error || loadFailed) && (
          <Notice variant="danger">{error ?? t('settings.viewAs.engineFailed')}</Notice>
        )}
        {admin && <Notice variant="info">{t('settings.viewAs.administrator')}</Notice>}
        <h3 className="ss-sub-title">{t('settings.viewAs.resultTitle')}</h3>
        {!entries ? (
          <p className="ss-na">{t('settings.viewAs.loading')}</p>
        ) : shown.length === 0 ? (
          <p className="ss-na">{t('settings.viewAs.none')}</p>
        ) : (
          <ul className="ss-list" aria-label={t('settings.viewAs.resultLabel')}>
            {shown.map(({ channel: c }) => (
              <li
                key={c.id}
                className={`ss-row${c.type !== 4 && c.parent_id ? ' ss-tree-child' : ''}`}
                data-testid={`view-as-${c.id}`}
              >
                <span aria-hidden="true">{ICON[c.type] ?? '#'}</span>
                <span className="ss-grow">{c.name ?? c.id}</span>
              </li>
            ))}
          </ul>
        )}
        {entries && (
          <p className="ss-sub">
            {t('settings.viewAs.count', {
              shown: shown.filter((e) => e.channel.type !== 4).length,
              total: entries.filter((e) => e.channel.type !== 4).length,
            })}
          </p>
        )}
      </div>
    </Modal>
  );
}
