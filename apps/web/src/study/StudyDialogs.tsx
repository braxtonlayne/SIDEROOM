import { Button, Modal, Notice, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { presignGet } from '../storage/api';
import { playableKind, NATIVE_VIDEO } from '../storage/shelves/api';
import { CbzReader } from '../storage/shelves/CbzReader';
import { PresignedReader } from '../storage/shelves/PresignedReader';
import { ShelfPlayer } from '../storage/shelves/ShelfPlayer';
import { extOf, typeOf } from './fileTypes';
import { coverChoices, noteFromForm, type FileNote } from './notes';
import type { Opened } from './rails';

const NATIVE_AUDIO = new Set(['mp3', 'flac', 'ogg', 'oga', 'opus', 'm4a', 'aac', 'wav']);

/** Opens a file: plays or reads it in the browser when it can, else offers the file's own short-lived link. */
export function StudyViewer({
  mountId,
  fileKey,
  title,
  resume,
  onClose,
  onProgress,
}: {
  mountId: string;
  fileKey: string;
  title: string;
  resume?: Opened;
  onClose: () => void;
  onProgress: (at: number, of: number) => void;
}) {
  const t = useT();
  const ext = extOf(fileKey);
  const type = typeOf(fileKey);
  const native = NATIVE_VIDEO.has(ext) || NATIVE_AUDIO.has(ext);
  const kind = playableKind(ext, native);
  return (
    <Modal open onClose={onClose} title={title} size="lg">
      <div className="stu-viewer">
        {(kind === 'video' || kind === 'audio') && (
          <ShelfPlayer
            mountId={mountId}
            objectKey={fileKey}
            kind={kind}
            title={title}
            startAt={resume?.at}
            onTime={onProgress}
          />
        )}
        {kind === 'cbz' && (
          <CbzReader
            mountId={mountId}
            objectKey={fileKey}
            title={title}
            startPage={resume?.at}
            onPage={(i, total) => onProgress(i, total)}
          />
        )}
        {(kind === 'epub' || kind === 'pdf') && (
          <PresignedReader
            mountId={mountId}
            objectKey={fileKey}
            kind={kind}
            title={title}
            seriesId={`study-${mountId}`}
          />
        )}
        {kind === 'none' && type === 'images' && (
          <ImageView mountId={mountId} fileKey={fileKey} title={title} />
        )}
        {kind === 'none' && type !== 'images' && (
          <>
            <Notice variant="info">{t('study.viewer.noPreview')}</Notice>
            <LinkButton mountId={mountId} fileKey={fileKey} />
          </>
        )}
      </div>
    </Modal>
  );
}

function ImageView({
  mountId,
  fileKey,
  title,
}: {
  mountId: string;
  fileKey: string;
  title: string;
}) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    presignGet(mountId, fileKey)
      .then((p) => alive && setUrl(p.url))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [mountId, fileKey]);
  if (failed) return <Notice variant="danger">{t('study.viewer.failed')}</Notice>;
  if (!url) return <Spinner label={t('study.loading')} />;
  return <img className="stu-image" src={url} alt={title} />;
}

/** "Open file": asks for a fresh link at the click and opens it; nothing is stored. */
function LinkButton({ mountId, fileKey }: { mountId: string; fileKey: string }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  return (
    <>
      <Button
        loading={busy}
        onClick={() => {
          setBusy(true);
          setFailed(false);
          presignGet(mountId, fileKey)
            .then((p) => window.open(p.url, '_blank', 'noopener,noreferrer'))
            .catch(() => setFailed(true))
            .finally(() => setBusy(false));
        }}
      >
        {t('study.viewer.openFile')}
      </Button>
      {failed && <Notice variant="danger">{t('study.viewer.failed')}</Notice>}
    </>
  );
}

/** The owner's editor for one file: a custom title, a cover (an image of the same folder) and a note. */
export function NoteEditor({
  fileKey,
  current,
  images,
  saving,
  error,
  onSave,
  onClose,
}: {
  fileKey: string;
  current: FileNote | undefined;
  /** Keys of the images in the folder being shown. */
  images: readonly string[];
  saving: boolean;
  error: string | null;
  onSave: (n: FileNote | null) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [title, setTitle] = useState(current?.title ?? '');
  const [note, setNote] = useState(current?.note ?? '');
  const [cover, setCover] = useState(current?.cover_key ?? '');
  const choices = coverChoices(images);
  if (cover && !choices.includes(cover)) choices.unshift(cover);
  return (
    <Modal
      open
      onClose={onClose}
      title={t('study.edit.title')}
      description={t('study.edit.hint')}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('study.edit.cancel')}
          </Button>
          <Button
            loading={saving}
            onClick={() => onSave(noteFromForm(fileKey, { title, note, cover }))}
          >
            {t('study.edit.save')}
          </Button>
        </>
      }
    >
      <div className="stu-form">
        <TextInput
          label={t('study.edit.fieldTitle')}
          value={title}
          maxLength={300}
          onChange={(e) => setTitle(e.target.value)}
        />
        <label className="stu-field">
          <span>{t('study.edit.fieldNote')}</span>
          <textarea
            className="stu-textarea"
            value={note}
            maxLength={2000}
            rows={4}
            onChange={(e) => setNote(e.target.value)}
          />
        </label>
        <label className="stu-field">
          <span>{t('study.edit.fieldCover')}</span>
          <select value={cover} onChange={(e) => setCover(e.target.value)}>
            <option value="">{t('study.edit.coverOwn')}</option>
            {choices.map((k) => (
              <option key={k} value={k}>
                {k.split('/').pop()}
              </option>
            ))}
          </select>
        </label>
        {error && <Notice variant="danger">{error}</Notice>}
      </div>
    </Modal>
  );
}
