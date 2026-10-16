import { Button, Modal, Spinner, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { getPrivacy, patchPrivacy, type PrivacySettings } from '../../settings/user/api';
import { useOpenUserSettings } from '../../settings/user/nav';
import { useGuild } from '../data';
import { useLoungeUi } from '../store';

/** Whether DMs from this server's members are welcome: the person's override, else their account-wide choice. */
export const serverDmsAllowed = (p: PrivacySettings, guildId: string) =>
  p.server_dm_overrides?.[guildId] ?? p.allow_server_member_dms;

/**
 * "Allow direct messages from server members" for one server (LNG-GLD-025), over the existing per-server override of
 * `GET/PATCH /users/@me/privacy`: turning it off writes `{server_dm_overrides: {<id>: false}}`, turning it back on
 * clears the override (or sets `true` when the account-wide switch is off). The Safety settings list reads the same
 * field, so it shows the change the next time it is opened.
 */
export function ServerDmToggle({ guildId }: { guildId: string }) {
  const t = useT();
  const [priv, setPriv] = useState<PrivacySettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    getPrivacy().then(
      (p) => live && setPriv(p),
      () => live && setError(t('lounge.serverDm.loadError')),
    );
    return () => {
      live = false;
    };
  }, [t]);

  const change = async (allow: boolean) => {
    if (!priv) return;
    setBusy(true);
    setError(null);
    // Back to the default when the account-wide choice already says yes.
    const value = allow && priv.allow_server_member_dms ? null : allow;
    try {
      setPriv(await patchPrivacy({ server_dm_overrides: { [guildId]: value } }));
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.serverDm.saveError'));
    }
    setBusy(false);
  };

  if (!priv && !error) return <Spinner label={t('lounge.serverDm.loading')} />;
  return (
    <>
      {priv && (
        <Switch
          label={t('lounge.serverDm.label')}
          description={t('lounge.serverDm.hint')}
          checked={serverDmsAllowed(priv, guildId)}
          disabled={busy}
          onChange={(e) => void change(e.target.checked)}
        />
      )}
      {error && <p role="alert">{error}</p>}
    </>
  );
}

/** The server menu's "Privacy Settings": the per-server DM switch, and the way to the account-wide ones. */
export function ServerPrivacyDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const closeDialog = useLoungeUi((s) => s.closeDialog);
  const openUserSettings = useOpenUserSettings();
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.serverDm.title')}
      description={t('lounge.serverDm.subtitle', { name: guild?.name ?? '' })}
      footer={<Button onClick={onClose}>{t('lounge.serverDm.close')}</Button>}
    >
      <ServerDmToggle guildId={guildId} />
      <p className="lng-muted">
        <Button
          variant="link"
          onClick={() => {
            closeDialog();
            openUserSettings('privacy');
          }}
        >
          {t('lounge.serverDm.moreSettings')}
        </Button>
      </p>
    </Modal>
  );
}
