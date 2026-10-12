import { Button, ConfirmDialog, EmptyState } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useGuild, useStore } from '../../../store';
import * as api from '../api';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, SectionHeader, useAction } from '../ui';

/** LNG-SSET-072: typed-name confirmation, owner only. */
export function DeleteServerSection({ guildId, viewer }: SectionProps) {
  const { guild } = useGuild(guildId);
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const action = useAction();

  if (!viewer.isOwner) return <EmptyState title="Only the server owner can delete this server" />;

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
      <SectionHeader title="Delete Server" />
      <div className="ss-danger-zone">
        <p className="ss-sub">
          Deleting <strong>{guild?.name}</strong> removes every channel and member from the server.
          Messages are kept for the retention window and then erased. This cannot be undone.
        </p>
        <ActionStatus error={action.error} />
        <div>
          <Button variant="danger" onClick={() => setOpen(true)}>
            Delete server
          </Button>
        </div>
      </div>
      <ConfirmDialog
        open={open}
        danger
        title={`Delete ${guild?.name ?? 'server'}`}
        description="Type the server name to confirm. This cannot be undone."
        confirmLabel="Delete server"
        requireText={guild?.name ?? ''}
        loading={action.busy}
        onConfirm={() => void confirm()}
        onCancel={() => setOpen(false)}
      />
    </>
  );
}
