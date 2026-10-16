import { Button, Modal, TextInput } from '@sideroom/ui';
import { useRef, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useLoungeGotoThread } from '../nav';
import { UploadTray } from '../msg/UploadTray';
import { MAX_FILES, uploadFiles, type PendingFile } from '../msg/upload';
import { useStore } from '../../store';
import { formatBytes } from '../msg/format';
import { THREAD_NAME_MAX } from '../threads/CreateThreadDialog';
import { useThreads } from '../threads/threadStore';
import type { ForumTag } from './forumLogic';

const MAX_APPLIED = 5;

let fileCounter = 0;

/** New Post: title, tags, first message and files (LNG-FRM-004, H-38). */
export function NewPostDialog({
  guildId,
  channelId,
  tags,
  requireTag,
  onClose,
}: {
  guildId: string;
  channelId: string;
  tags: ForumTag[];
  requireTag: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [files, setFiles] = useState<PendingFile[]>([]);
  const picker = useRef<HTMLInputElement>(null);
  const abort = useRef<AbortController | null>(null);
  const openThread = useLoungeGotoThread();
  // The size the server will accept for this viewer in the server (D-233, D-238).
  const maxUpload = useStore((s) => s.guilds[guildId]?.sr_max_upload_bytes);

  const patchFile = (id: string, patch: Partial<PendingFile>) =>
    setFiles((cur) => cur.map((f) => (f.id === id ? { ...f, ...patch } : f)));
  const addFiles = (picked: File[]) => {
    const fits = maxUpload === undefined ? picked : picked.filter((f) => f.size <= maxUpload);
    if (fits.length < picked.length)
      setError(t('lounge.newPost.fileTooLarge', { max: formatBytes(maxUpload ?? 0) }));
    else setError(null);
    setFiles((cur) => [
      ...cur,
      ...fits.slice(0, Math.max(0, MAX_FILES - cur.length)).map((file): PendingFile => ({
        id: `p${Date.now().toString(36)}${fileCounter++}`,
        file,
        name: file.name || 'file',
        spoiler: false,
        description: '',
        progress: 0,
        status: 'queued',
      })),
    ]);
  };

  const toggle = (id: string) =>
    setPicked((cur) =>
      cur.includes(id)
        ? cur.filter((x) => x !== id)
        : cur.length >= MAX_APPLIED
          ? cur
          : [...cur, id],
    );

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!title.trim() || (!content.trim() && files.length === 0)) {
      setError(t('lounge.newPost.required'));
      return;
    }
    if (requireTag && picked.length === 0) {
      setError(t('lounge.newPost.tagRequired'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // The files go to the forum's own upload slots first; the post then carries their ids.
      let attachments: { id: string; uploaded_filename: string; description?: string }[] = [];
      if (files.length > 0) {
        abort.current = new AbortController();
        const done = await uploadFiles(channelId, files, {
          onUpdate: patchFile,
          signal: abort.current.signal,
        });
        attachments = done.map((d) => ({
          id: d.id,
          uploaded_filename: d.uploadedFilename,
          ...(d.description ? { description: d.description } : {}),
        }));
      }
      const w = await apiFetch<{ id: string; parent_id?: string }>(
        `/channels/${channelId}/threads`,
        {
          method: 'POST',
          json: {
            name: title.trim(),
            message: {
              content: content.trim(),
              ...(attachments.length ? { attachments } : {}),
            },
            ...(picked.length ? { applied_tags: picked } : {}),
          },
          retries: 0,
        },
      );
      useThreads.getState().upsert([{ ...w, parent_id: w.parent_id ?? channelId, member: {} }]);
      onClose();
      openThread(guildId, channelId, w.id);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('lounge.newPost.forbidden')
          : t('lounge.newPost.error'),
      );
      setBusy(false);
    }
  };

  return (
    <Modal open onClose={onClose} size="md" title={t('lounge.newPost.title')}>
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        <TextInput
          label={t('lounge.newPost.titleLabel')}
          value={title}
          maxLength={THREAD_NAME_MAX}
          autoFocus
          onChange={(e) => setTitle(e.target.value)}
        />
        {tags.length > 0 && (
          <div role="group" aria-label={t('lounge.newPost.tags')} className="lng-forum-chips">
            {tags.map((tag) => (
              <button
                key={tag.id}
                type="button"
                className="lng-tag lng-tag-filter"
                aria-pressed={picked.includes(tag.id)}
                onClick={() => toggle(tag.id)}
              >
                {tag.emoji ?? ''} {tag.name}
              </button>
            ))}
          </div>
        )}
        <label className="lng-field-label" htmlFor="new-post-body">
          {t('lounge.newPost.bodyLabel')}
        </label>
        <textarea
          id="new-post-body"
          className="lng-post-textarea"
          rows={6}
          value={content}
          maxLength={2000}
          onChange={(e) => setContent(e.target.value)}
        />
        <UploadTray
          files={files}
          busy={busy}
          onChange={patchFile}
          onRemove={(id) => {
            if (busy) abort.current?.abort();
            else setFiles((cur) => cur.filter((f) => f.id !== id));
          }}
        />
        <input
          ref={picker}
          type="file"
          multiple
          hidden
          data-testid="new-post-files"
          onChange={(e) => {
            addFiles(Array.from(e.target.files ?? []));
            e.target.value = '';
          }}
        />
        <div>
          <Button
            variant="secondary"
            size="sm"
            disabled={busy || files.length >= MAX_FILES}
            onClick={() => picker.current?.click()}
          >
            {t('lounge.newPost.attach')}
          </Button>
        </div>
        {error && (
          <p className="lng-muted" role="alert">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.createThread.cancel')}
          </Button>
          <Button type="submit" loading={busy}>
            {t('lounge.newPost.submit')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
