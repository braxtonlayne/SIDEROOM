import { Button, Modal } from '@sideroom/ui';
import type { ReactNode } from 'react';

/** Placeholder modal for flows owned by other inventory items (Create Server, Directory, ...). */
export function ComingSoon({
  open,
  onClose,
  title,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children?: ReactNode;
}) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      size="sm"
      footer={<Button onClick={onClose}>Got it</Button>}
    >
      {children ?? <p>This part of the Lounge is not built yet.</p>}
    </Modal>
  );
}
