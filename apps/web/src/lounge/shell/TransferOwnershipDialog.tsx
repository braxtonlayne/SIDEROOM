import { ConfirmDialog } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useStore } from '../../store';
import { useGuild, useMembers } from '../data';

/**
 * Transfer Ownership (LNG-GLD-023): `PATCH /guilds/{id}` with `owner_id`, which only the current owner may
 * send. The previous owner keeps their roles and loses owner powers at once.
 */
export function TransferOwnershipDialog({
  guildId,
  userId,
  onClose,
}: {
  guildId: string;
  userId: string;
  onClose: () => void;
}) {
  const guild = useGuild(guildId);
  const member = useMembers(guildId).find((m) => m.user.id === userId);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!guild || !member) return null;
  const name = member.nick ?? member.user.displayName;

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await apiFetch<unknown>(`/guilds/${guildId}`, {
        method: 'PATCH',
        json: { owner_id: userId },
        retries: 0,
      });
      useStore.getState().applyDispatch('GUILD_UPDATE', { id: guildId, owner_id: userId });
      onClose();
    } catch (e) {
      setError(
        e instanceof ApiError && e.status === 403
          ? 'Only the server owner can transfer ownership.'
          : 'Could not transfer ownership. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title="Transfer Ownership"
      description={
        <>
          Make <strong>{name}</strong> the owner of <strong>{guild.name}</strong>? You will no
          longer be able to delete the server or transfer it again, and only {name} can undo this.
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel="Transfer Ownership"
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
