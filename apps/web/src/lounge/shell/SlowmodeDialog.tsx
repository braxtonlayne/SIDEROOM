import { Button, Modal } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useChannels } from '../data';
import { SlowmodeForm } from '../features/SlowmodeForm';

/** "Apply Slowmode Preset" from a channel's context menu (FTR-SLOW-001, G-74). */
export function SlowmodeDialog({
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
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.slowmode.title', { name: channel?.name ?? '' })}
      description={t('lounge.slowmode.description')}
      footer={
        <Button variant="ghost" onClick={onClose}>
          {t('lounge.slowmode.close')}
        </Button>
      }
    >
      <SlowmodeForm guildId={guildId} channelIds={[channelId]} />
    </Modal>
  );
}
