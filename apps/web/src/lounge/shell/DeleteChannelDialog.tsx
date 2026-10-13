import { ConfirmDialog } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useChannels } from '../data';
import { useLoungeRoute } from '../nav';
import { channelErrorText, deleteChannelAndForget } from './channelActions';

/** Delete Channel / Delete Category confirm (LNG-CHN-013, LNG-CHN-014). */
export function DeleteChannelDialog({
  guildId,
  channelId,
  onClose,
}: {
  guildId: string;
  channelId: string;
  onClose: () => void;
}) {
  const channel = useChannels(guildId).find((c) => c.id === channelId);
  const { channelId: current } = useLoungeRoute();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!channel) return null;
  const cat = channel.type === 'category';

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await deleteChannelAndForget(guildId, channelId);
      onClose();
      if (current === channelId)
        void navigate({ to: '/channels/$guildId', params: { guildId }, replace: true });
    } catch (e) {
      setError(channelErrorText(e, 'Could not delete it. Try again.'));
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title={cat ? 'Delete Category' : 'Delete Channel'}
      description={
        <>
          {cat
            ? `Delete the category ${channel.name}? Its channels are kept and move to the top level.`
            : `Are you sure you want to delete #${channel.name}? This cannot be undone.`}
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel={cat ? 'Delete Category' : 'Delete Channel'}
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
