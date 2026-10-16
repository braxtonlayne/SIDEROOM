import { Button, Modal, Spinner, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useGuild } from '../data';
import { getSecurity, patchSecurity, type SecuritySettings } from './joinPipeline';

/**
 * Security Actions (LNG-GLD-034): the two quick switches moderators reach for during a raid. "Pause DMs" is the
 * server's DM filter (D-224): messages from this server's members go to Message Requests instead of the inbox.
 */
export function SecurityActionsDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const [s, setS] = useState<SecuritySettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    getSecurity(guildId).then(
      (v) => live && setS(v),
      () => live && setError(t('lounge.securityActions.loadError')),
    );
    return () => {
      live = false;
    };
  }, [guildId, t]);

  const flip = async (patch: Partial<SecuritySettings>) => {
    setBusy(true);
    setError(null);
    try {
      setS(await patchSecurity(guildId, patch));
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.securityActions.saveError'),
      );
    }
    setBusy(false);
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.securityActions.title')}
      description={t('lounge.securityActions.subtitle', { name: guild?.name ?? '' })}
      footer={<Button onClick={onClose}>{t('lounge.securityActions.close')}</Button>}
    >
      {!s && !error && <Spinner label={t('lounge.securityActions.loading')} />}
      {s && (
        <>
          <Switch
            label={t('lounge.securityActions.pauseInvites')}
            description={t('lounge.securityActions.pauseInvitesHint')}
            checked={s.invites_paused}
            disabled={busy}
            onChange={(e) => void flip({ invites_paused: e.target.checked })}
          />
          <Switch
            label={t('lounge.securityActions.pauseDms')}
            description={t('lounge.securityActions.pauseDmsHint')}
            checked={!!s.dm_spam_filter}
            disabled={busy}
            onChange={(e) => void flip({ dm_spam_filter: e.target.checked })}
          />
        </>
      )}
      {error && <p role="alert">{error}</p>}
    </Modal>
  );
}
