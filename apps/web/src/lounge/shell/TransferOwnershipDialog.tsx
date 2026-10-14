import { ConfirmDialog } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import { useGuild, useMembers } from '../data';

/** Stand-ins for the bold name and server in the translated sentence (private-use code points). */
const NAME_MARK = String.fromCharCode(0xe000);
const GUILD_MARK = String.fromCharCode(0xe001);

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
  const t = useT();
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
          ? t('lounge.transferOwnership.forbidden')
          : t('lounge.transferOwnership.error'),
      );
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title={t('lounge.transferOwnership.title')}
      description={
        <>
          {t('lounge.transferOwnership.body', { name: NAME_MARK, guild: GUILD_MARK, who: name })
            .split(new RegExp(`(${NAME_MARK}|${GUILD_MARK})`))
            .map((part, i) =>
              part === NAME_MARK ? (
                <strong key={i}>{name}</strong>
              ) : part === GUILD_MARK ? (
                <strong key={i}>{guild.name}</strong>
              ) : (
                part
              ),
            )}
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel={t('lounge.transferOwnership.title')}
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
