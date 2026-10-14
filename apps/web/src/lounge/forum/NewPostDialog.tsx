import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useLoungeGotoThread } from '../nav';
import { THREAD_NAME_MAX } from '../threads/CreateThreadDialog';
import { useThreads } from '../threads/threadStore';
import type { ForumTag } from './forumLogic';

const MAX_APPLIED = 5;

/** New Post: title, tags, first message (LNG-FRM-004). Attachments are not accepted by the server yet. */
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
  const openThread = useLoungeGotoThread();

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
    if (!title.trim() || !content.trim()) {
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
      const w = await apiFetch<{ id: string; parent_id?: string }>(
        `/channels/${channelId}/threads`,
        {
          method: 'POST',
          json: {
            name: title.trim(),
            message: { content: content.trim() },
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
