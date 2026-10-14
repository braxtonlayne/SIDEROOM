// Invite / share link (spec section 2): the party link, copyable. Private parties carry the invite code in it.
import { Button, Modal, TextInput, useToast } from '@sideroom/ui';
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
  const toast = useToast();
  const link = partyLink(window.location.origin, party);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link);
      toast.show({
        title: 'Link copied',
        description: 'Send it to the people you want to watch with.',
      });
    } catch {
      toast.show({ title: 'Could not copy the link', variant: 'warning' });
    }
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Invite people"
      description={
        party.binding === 'private'
          ? 'Anyone with this link can join the party. Keep it to people you trust.'
          : 'Everyone who can see the channel can join from there. This link opens the party for them.'
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Close
          </Button>
          <Button onClick={() => void copy()}>Copy link</Button>
        </>
      }
    >
      <TextInput
        label="Party link"
        readOnly
        value={link}
        onFocus={(e) => e.currentTarget.select()}
      />
    </Modal>
  );
}
