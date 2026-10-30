/**
 * UTL-IMG-001: the studio in a dialog, for the two places an image already exists: the composer's attachment
 * tray ("Edit": `onUse` replaces the pending file) and an image in a message ("Open in studio": the studio
 * works on a COPY and offers a download; the message and its attachment are never changed). Loaded lazily by
 * both callers, so the studio costs nothing until it is opened.
 */
import { Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { ImageLoader } from './ImageLoader';
import { StudioEditor } from './StudioEditor';

export interface StudioModalProps {
  onClose: () => void;
  /** The image to edit (the composer's pending file). */
  source?: Blob | undefined;
  /** Or a URL to copy the image from (a message attachment). */
  sourceUrl?: string | undefined;
  /** Name for the saved file, without extension. */
  name?: string | undefined;
  /** Receives the edited file. Absent: the studio only offers a download. */
  onUse?: ((file: File) => void) | undefined;
  /** "copy" labels the hand-back button "Save as a copy". */
  mode?: 'edit' | 'copy';
}

export default function StudioModal({
  onClose,
  source,
  sourceUrl,
  name,
  onUse,
  mode = 'edit',
}: StudioModalProps) {
  const t = useT();
  const [blob, setBlob] = useState<Blob | undefined>(source);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (source || !sourceUrl) return;
    let live = true;
    fetch(sourceUrl)
      .then((r) => (r.ok ? r.blob() : Promise.reject(new Error(String(r.status)))))
      .then((b) => live && setBlob(b))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [source, sourceUrl]);

  return (
    <Modal open onClose={onClose} title={t('studio.modal.title')} size="lg">
      {mode === 'copy' && <p className="sr-tl-muted">{t('studio.open.copyNote')}</p>}
      {failed ? (
        <p role="alert" className="sr-st-error">
          {t('studio.open.failed')}
        </p>
      ) : (
        <ImageLoader source={blob}>
          {(bitmap, base) => (
            <StudioEditor
              bitmap={bitmap}
              baseName={name ?? base}
              initialMode="crop"
              {...(onUse ? { onUse } : {})}
              useLabel={mode === 'copy' ? 'copy' : 'edited'}
            />
          )}
        </ImageLoader>
      )}
    </Modal>
  );
}
