import { ConfirmDialog } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../i18n';
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
  const t = useT();
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
      setError(channelErrorText(e, t('lounge.deleteChannel.error')));
      setBusy(false);
    }
  };

  return (
    <ConfirmDialog
      open
      danger
      title={cat ? t('lounge.deleteChannel.categoryTitle') : t('lounge.deleteChannel.channelTitle')}
      description={
        <>
          {cat
            ? t('lounge.deleteChannel.categoryBody', { name: channel.name })
            : t('lounge.deleteChannel.channelBody', { name: channel.name })}
          {error && (
            <span role="alert" className="lng-form-error">
              {error}
            </span>
          )}
        </>
      }
      confirmLabel={
        cat ? t('lounge.deleteChannel.categoryTitle') : t('lounge.deleteChannel.channelTitle')
      }
      loading={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}
