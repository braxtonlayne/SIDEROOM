// Invite / share link (spec section 2): the party link, copyable. Private parties carry the invite code in it.
import { Button, Modal, TextInput, useToast } from '@sideroom/ui';
import { useT } from '../i18n';
import { partyLink } from './api';
import type { Party } from './types';

export function ShareDialog({
  open,
  onClose,
  party,
}: {
  open: boolean;
  onClose: () => void;
  party: Pick<Party, 'id' | 'invite_code' | 'binding'>;
}) {
  const t = useT();
  const toast = useToast();
  const link = partyLink(window.location.origin, party);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link);
      toast.show({
        title: t('watchparty.share.copied'),
        description: t('watchparty.share.copiedDescription'),
      });
    } catch {
      toast.show({ title: t('watchparty.share.copyFailed'), variant: 'warning' });
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('watchparty.share.title')}
      description={
        party.binding === 'private'
          ? t('watchparty.share.descriptionPrivate')
          : t('watchparty.share.descriptionChannel')
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('watchparty.share.close')}
          </Button>
          <Button onClick={() => void copy()}>{t('watchparty.share.copyLink')}</Button>
        </>
      }
    >
      <TextInput
        label={t('watchparty.share.linkLabel')}
        readOnly
        value={link}
        onFocus={(e) => e.currentTarget.select()}
      />
    </Modal>
  );
}
