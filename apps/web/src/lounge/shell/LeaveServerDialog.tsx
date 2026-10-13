import { ConfirmDialog, Modal, Button } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useStore } from '../../store';
import { HOME } from '../channels';
import { useCurrentUser, useGuild } from '../data';
import { leaveGuild } from './api';

/** Leave Server confirm (LNG-GLD-027). The owner is told to transfer ownership first. */
export function LeaveServerDialog({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const guild = useGuild(guildId);
  const me = useCurrentUser();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!guild) return null;

  if (guild.ownerId === me.id)
    return (
      <Modal
        open
        onClose={onClose}
        size="sm"
        title={`Leave ${guild.name}`}
        footer={<Button onClick={onClose}>Got it</Button>}
      >
        <p>
          You own this server, so you cannot leave it yet. Transfer ownership to another member in
          Server Settings first, or delete the server there.
        </p>
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
      setError(
        e instanceof ApiError && e.message ? e.message : 'Could not leave the server. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title={`Leave ${guild.name}`}
      description={
        <>
          Are you sure you want to leave <strong>{guild.name}</strong>? You will not be able to
          rejoin unless you are invited again.
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel="Leave Server"
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
