import { Modal } from '@sideroom/ui';

/** Trailer in a modal. The player agent's engine is not needed: a trailer is one short progressive file. */
export function TrailerModal({
  open,
  onClose,
  title,
  src,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  src: string;
}) {
  return (
    <Modal open={open} onClose={onClose} title={`${title}: trailer`} size="lg">
      {/* Trailers have no caption tracks of their own. */}
      <video className="cin-trailer" src={src} controls autoPlay playsInline />
    </Modal>
  );
}
