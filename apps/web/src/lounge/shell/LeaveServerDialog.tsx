import { ConfirmDialog, Modal, Button } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import { HOME } from '../channels';
import { useCurrentUser, useGuild } from '../data';
import { leaveGuild } from './api';

/** Leave Server confirm (LNG-GLD-027). The owner is told to transfer ownership first. */
export function LeaveServerDialog({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const t = useT();
  const guild = useGuild(guildId);
  const me = useCurrentUser();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!guild) return null;
  const [leavePre, leavePost] = t('lounge.leaveServer.confirmBody', { name: '\u0001' }).split(
    '\u0001',
  );

  if (guild.ownerId === me.id)
    return (
      <Modal
        open
        onClose={onClose}
        size="sm"
        title={t('lounge.leaveServer.title', { name: guild.name })}
        footer={<Button onClick={onClose}>{t('lounge.leaveServer.gotIt')}</Button>}
      >
        <p>{t('lounge.leaveServer.ownerBody')}</p>
      </Modal>
    );

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await leaveGuild(guildId);
      useStore.getState().applyDispatch('GUILD_DELETE', { id: guildId });
      onClose();
      void navigate({ to: '/channels/$guildId', params: { guildId: HOME } });
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.leaveServer.error'));
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title={t('lounge.leaveServer.title', { name: guild.name })}
      description={
        <>
          {leavePre}
          <strong>{guild.name}</strong>
          {leavePost}
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel={t('lounge.leaveServer.confirm')}
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
