// New folder, rename or move, and delete in the bucket browser (BYOS-BRW-001). Each is a small dialog that calls
// one route and reports the answer in words; the browser reloads the listing afterwards.
import { Button, ConfirmDialog, Modal, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { createFolder, deleteObjects, moveObjects } from '../api';
import { baseName, editablePath, folderKey, moveTarget } from './keys';
import { writeErrorText } from './writeErrors';

/** The "New folder" button and its dialog; `prefix` is the folder being viewed. */
export function NewFolder({
  mountId,
  prefix,
  onChanged,
}: {
  mountId: string;
  prefix: string;
  onChanged: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const close = () => {
    setOpen(false);
    setName('');
    setError(null);
  };
  const where = prefix === '' ? t('storage.write.upload.top') : prefix;
  const submit = async () => {
    const key = folderKey(prefix, name);
    if (key === null) {
      setError(t('storage.write.folder.noSlash'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await createFolder(mountId, key);
      close();
      onChanged();
    } catch (e) {
      setError(writeErrorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Button variant="secondary" onClick={() => setOpen(true)}>
        {t('storage.write.folder.button')}
      </Button>
      <Modal
        open={open}
        onClose={close}
        title={t('storage.write.folder.title')}
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={close}>
              {t('storage.write.cancel')}
            </Button>
            <Button disabled={busy || name.trim() === ''} onClick={() => void submit()}>
              {t('storage.write.folder.create')}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy && name.trim() !== '') void submit();
          }}
        >
          <TextInput
            label={t('storage.write.folder.name')}
            hint={t('storage.write.folder.help', { path: where })}
            value={name}
            maxLength={200}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </form>
        {error && <Notice variant="danger">{error}</Notice>}
      </Modal>
    </>
  );
}

/** Rename-or-move and delete for one row; `fileKey` ends in a slash for a folder. */
export function RowActions({
  mountId,
  fileKey,
  onChanged,
  onNote,
  onError,
}: {
  mountId: string;
  fileKey: string;
  onChanged: () => void;
  onNote: (text: string) => void;
  onError: (text: string) => void;
}) {
  const t = useT();
  const name = baseName(fileKey);
  const [moving, setMoving] = useState(false);
  const [typed, setTyped] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [busy, setBusy] = useState(false);

  const openMove = () => {
    setTyped(editablePath(fileKey));
    setProblem(null);
    setMoving(true);
  };
  const move = async () => {
    const to = moveTarget(fileKey, typed);
    if (to === null) {
      setProblem(t('storage.write.err.keyInvalid'));
      return;
    }
    setBusy(true);
    setProblem(null);
    try {
      await moveObjects(mountId, fileKey, to);
      setMoving(false);
      onNote(t('storage.write.move.done', { name }));
      onChanged();
    } catch (e) {
      setProblem(writeErrorText(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    setBusy(true);
    try {
      const r = await deleteObjects(mountId, fileKey);
      setDeleting(false);
      onNote(t('storage.write.delete.done', { count: r.count }));
      onChanged();
    } catch (e) {
      setDeleting(false);
      onError(writeErrorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Button
        size="sm"
        variant="secondary"
        aria-label={t('storage.write.move.action', { name })}
        onClick={openMove}
      >
        {t('storage.write.move.button')}
      </Button>
      <Button
        size="sm"
        variant="secondary"
        aria-label={t('storage.write.delete.action', { name })}
        onClick={() => setDeleting(true)}
      >
        {t('storage.write.delete.button')}
      </Button>
      <Modal
        open={moving}
        onClose={() => setMoving(false)}
        title={t('storage.write.move.title', { name })}
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setMoving(false)}>
              {t('storage.write.cancel')}
            </Button>
            <Button disabled={busy || typed.trim() === ''} onClick={() => void move()}>
              {t('storage.write.move.confirm')}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy && typed.trim() !== '') void move();
          }}
        >
          <TextInput
            label={t('storage.write.move.label')}
            hint={t('storage.write.move.help')}
            value={typed}
            maxLength={1024}
            onChange={(e) => setTyped(e.target.value)}
            autoFocus
          />
        </form>
        {problem && <Notice variant="danger">{problem}</Notice>}
      </Modal>
      <ConfirmDialog
        open={deleting}
        danger
        loading={busy}
        title={t('storage.write.delete.title', { name })}
        description={t(
          fileKey.endsWith('/')
            ? 'storage.write.delete.folderBody'
            : 'storage.write.delete.fileBody',
        )}
        confirmLabel={t('storage.write.delete.confirm')}
        cancelLabel={t('storage.write.cancel')}
        onConfirm={() => void remove()}
        onCancel={() => setDeleting(false)}
      />
    </>
  );
}
