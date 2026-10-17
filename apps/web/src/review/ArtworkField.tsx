// Artwork slot of a draft (REV-ED-003, K-11): pick an image, crop it to the slot's shape, upload it to
// `POST /submissions/{id}/artwork/{kind}`. The submitter uses it while the draft is editable; a reviewer who
// holds the claim uses it on a submission in review (D-255). "Pick a frame from the video" for stills is
// deferred.
import { Button, Notice } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { ImageCropDialog } from '../settings/user/ImageCropDialog';
import { imageProblem } from '../settings/user/imageCrop';
import * as rv from './api';
import { errorMessages } from './api';

export type ArtworkSlot = 'cover' | 'poster' | 'backdrop';

/** Shape and output size of each slot; the server bounds the stored image the same way. */
export const SLOT_SHAPE: Record<ArtworkSlot, { aspect: number; width: number }> = {
  cover: { aspect: 2 / 3, width: 600 },
  poster: { aspect: 2 / 3, width: 600 },
  backdrop: { aspect: 16 / 9, width: 1280 },
};

const LABEL: Record<ArtworkSlot, MessageKey> = {
  cover: 'review.artwork.cover',
  poster: 'review.artwork.poster',
  backdrop: 'review.artwork.backdrop',
};

export function ArtworkField({
  submissionId,
  slot,
  onSaved,
}: {
  submissionId: string;
  slot: ArtworkSlot;
  onSaved?: () => void;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [problems, setProblems] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const shape = SLOT_SHAPE[slot];
  return (
    <div className="rv-stack">
      <div className="rv-row">
        <Button variant="secondary" loading={busy} onClick={() => input.current?.click()}>
          {t('review.artwork.choose', { slot: t(LABEL[slot]) })}
        </Button>
        <input
          ref={input}
          type="file"
          hidden
          accept="image/png,image/jpeg,image/webp"
          aria-label={t('review.artwork.file', { slot: t(LABEL[slot]) })}
          onChange={(e) => {
            const f = e.target.files?.[0] ?? null;
            e.target.value = '';
            if (!f) return;
            const why = imageProblem(f);
            setDone(false);
            if (why) setProblems([why]);
            else {
              setProblems([]);
              setFile(f);
            }
          }}
        />
        {done && <span role="status">{t('review.artwork.saved')}</span>}
      </div>
      {problems.length > 0 && (
        <Notice variant="danger">
          {problems.map((p) => (
            <div key={p}>{p}</div>
          ))}
        </Notice>
      )}
      <ImageCropDialog
        open={file !== null}
        file={file}
        title={t('review.artwork.cropTitle', { slot: t(LABEL[slot]) })}
        aspect={shape.aspect}
        outWidth={shape.width}
        onCancel={() => setFile(null)}
        onApply={(uri) => {
          setFile(null);
          setBusy(true);
          setProblems([]);
          rv.setArtwork(submissionId, slot, uri).then(
            () => {
              setBusy(false);
              setDone(true);
              onSaved?.();
            },
            (e: unknown) => {
              setBusy(false);
              setProblems(errorMessages(e));
            },
          );
        }}
      />
    </div>
  );
}
