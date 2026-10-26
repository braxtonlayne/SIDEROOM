import { Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { storageErrorText } from '../../user/storage/errors';
import { STORAGE_GRANTS, putStorageGrants, type StorageGrant } from './grantsApi';

/**
 * The role editor's "Storage" group (BYOS-UI-001): five grants, saved at once through the grants route.
 * They are not permission bits, so they never appear in channel overwrites (a shelf belongs to no channel).
 */
export function StorageGrantsCard({
  guildId,
  roleId,
  grants,
  locked,
  onChanged,
}: {
  guildId: string;
  roleId: string;
  grants: string[];
  locked: boolean;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const [current, setCurrent] = useState<string[]>(grants);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const toggle = async (g: StorageGrant, on: boolean) => {
    const next = STORAGE_GRANTS.filter((x) => (x === g ? on : current.includes(x)));
    setBusy(true);
    setError(null);
    try {
      await putStorageGrants(guildId, roleId, next);
      setCurrent(next);
      await onChanged();
    } catch (e) {
      setError(storageErrorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="ss-card" aria-labelledby="sto-grants-h">
      <h3 id="sto-grants-h" className="ss-card-title">
        {t('settings.storageGrants.title')}
      </h3>
      <p className="ss-desc">{t('settings.storageGrants.description')}</p>
      {error && (
        <Notice variant="danger" title={t('settings.storageGrants.failed')}>
          {error}
        </Notice>
      )}
      {STORAGE_GRANTS.map((g) => (
        <Switch
          key={g}
          label={t(
            `settings.storageGrants.grant.${g.replace(/_(.)/g, (_, c: string) => c.toUpperCase())}`,
          )}
          checked={current.includes(g)}
          disabled={locked || busy}
          onChange={(e) => void toggle(g, e.target.checked)}
        />
      ))}
    </section>
  );
}
