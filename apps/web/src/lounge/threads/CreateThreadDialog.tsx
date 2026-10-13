import { Button, Modal, Select, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useStore } from '../../store';
import { useLoungeGotoThread } from '../nav';
import { useThreads } from './threadStore';

/** Discord's thread name limit (sr-chat THREAD_NAME_MAX). */
export const THREAD_NAME_MAX = 100;

export const ARCHIVE_CHOICES = [
  { value: '60', label: '1 hour' },
  { value: '1440', label: '24 hours' },
  { value: '4320', label: '3 days' },
  { value: '10080', label: '1 week' },
] as const;

/** First line of a message, trimmed to a thread name. */
export function suggestThreadName(content: string): string {
  const line =
    content
      .split('\n')[0]
      ?.replace(/[*_~`|>#]/g, '')
      .trim() ?? '';
  return (line || 'New Thread').slice(0, THREAD_NAME_MAX);
}

/** Create Thread from a message (LNG-MSGUI-025/026): `POST /channels/{c}/messages/{m}/threads`. */
export function CreateThreadDialog({
  guildId,
  channelId,
  messageId,
  onClose,
}: {
  guildId: string;
  channelId: string;
  /** Absent: a thread that does not grow from a message. */
  messageId?: string | undefined;
  onClose: () => void;
}) {
  const message = useStore((s) => (messageId ? s.messages[messageId] : undefined));
  const [name, setName] = useState(() => suggestThreadName(message?.content ?? ''));
  const [archive, setArchive] = useState('1440');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const openThread = useLoungeGotoThread();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Give the thread a name.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const body = { name: trimmed, auto_archive_duration: Number(archive) };
      const t = messageId
        ? await useThreads.getState().createFromMessage(channelId, messageId, body)
        : await useThreads.getState().createStandalone(channelId, body);
      onClose();
      openThread(guildId, channelId, t.id);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? 'You cannot start threads in this channel.'
          : err instanceof ApiError && err.status === 400
            ? 'That message already has a thread, or the name is not allowed.'
            : 'Could not create the thread. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <Modal open onClose={onClose} size="sm" title="Create Thread">
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Thread name"
          value={name}
          maxLength={THREAD_NAME_MAX}
          autoFocus
          error={error}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="lng-field-label" htmlFor="thread-archive">
          Hide after inactivity
        </label>
        <Select
          id="thread-archive"
          value={archive}
          onValueChange={setArchive}
          options={ARCHIVE_CHOICES.map((c) => ({ value: c.value, label: c.label }))}
        />
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Create Thread
          </Button>
        </div>
      </form>
    </Modal>
  );
}
