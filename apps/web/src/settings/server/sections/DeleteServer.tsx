import { Button, ConfirmDialog, EmptyState } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { useGuild, useStore } from '../../../store';
import * as api from '../api';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, SectionHeader, useAction } from '../ui';

/** LNG-SSET-072: typed-name confirmation, owner only. */
export function DeleteServerSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const { guild } = useGuild(guildId);
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const action = useAction();

  if (!viewer.isOwner) return <EmptyState title={t('settings.deleteServer.ownerOnly')} />;

  const confirm = async () => {
    const ok = await action.run(async () => {
      await api.deleteGuild(guildId);
      return true;
    });
    if (ok) {
      useStore.getState().applyDispatch('GUILD_DELETE', { id: guildId });
      setOpen(false);
      void navigate({ to: '/channels/$guildId', params: { guildId: '@me' } });
    }
  };

  return (
    <>
      <SectionHeader title={t('settings.deleteServer.title')} />
      <div className="ss-danger-zone">
        <p className="ss-sub">
          {t('settings.deleteServer.warningPrefix')} <strong>{guild?.name}</strong>{' '}
          {t('settings.deleteServer.warningSuffix')}
        </p>
        <ActionStatus error={action.error} />
        <div>
          <Button variant="danger" onClick={() => setOpen(true)}>
            {t('settings.deleteServer.button')}
          </Button>
        </div>
      </div>
      <ConfirmDialog
        open={open}
        danger
        title={t('settings.deleteServer.dialogTitle', {
          name: guild?.name ?? t('settings.deleteServer.defaultName'),
        })}
        description={t('settings.deleteServer.dialogDescription')}
        confirmLabel={t('settings.deleteServer.button')}
        requireText={guild?.name ?? ''}
        loading={action.busy}
        onConfirm={() => void confirm()}
        onCancel={() => setOpen(false)}
      />
    </>
  );
}
