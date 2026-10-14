import { Button, Modal, Select, Switch, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { t, useT, type MessageKey } from '../../i18n';
import { useStore } from '../../store';
import { useLoungeGotoThread } from '../nav';
import { useThreads } from './threadStore';

/** Discord's thread name limit (sr-chat THREAD_NAME_MAX). */
export const THREAD_NAME_MAX = 100;

export const ARCHIVE_CHOICES = [
  { value: '60', labelKey: 'lounge.createThread.archive1h' },
  { value: '1440', labelKey: 'lounge.createThread.archive24h' },
  { value: '4320', labelKey: 'lounge.createThread.archive3d' },
  { value: '10080', labelKey: 'lounge.createThread.archive1w' },
] as const satisfies readonly { value: string; labelKey: MessageKey }[];

/** First line of a message, trimmed to a thread name. */
export function suggestThreadName(content: string): string {
  const line =
    content
      .split('\n')[0]
      ?.replace(/[*_~`|>#]/g, '')
      .trim() ?? '';
  return (line || t('lounge.createThread.defaultName')).slice(0, THREAD_NAME_MAX);
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
  const t = useT();
  const message = useStore((s) => (messageId ? s.messages[messageId] : undefined));
  const [name, setName] = useState(() => suggestThreadName(message?.content ?? ''));
  const [archive, setArchive] = useState('1440');
  const [isPrivate, setIsPrivate] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const openThread = useLoungeGotoThread();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t('lounge.createThread.nameRequired'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const body = { name: trimmed, auto_archive_duration: Number(archive) };
      const created = messageId
        ? await useThreads.getState().createFromMessage(channelId, messageId, body)
        : await useThreads.getState().createStandalone(channelId, { ...body, private: isPrivate });
      onClose();
      openThread(guildId, channelId, created.id);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('lounge.createThread.forbidden')
          : err instanceof ApiError && err.status === 400
            ? t('lounge.createThread.badRequest')
            : t('lounge.createThread.error'),
      );
      setBusy(false);
    }
  };

  return (
    <Modal open onClose={onClose} size="sm" title={t('lounge.createThread.title')}>
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        <TextInput
          label={t('lounge.createThread.nameLabel')}
          value={name}
          maxLength={THREAD_NAME_MAX}
          autoFocus
          error={error}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="lng-field-label" htmlFor="thread-archive">
          {t('lounge.createThread.hideAfter')}
        </label>
        <Select
          id="thread-archive"
          value={archive}
          onValueChange={setArchive}
          options={ARCHIVE_CHOICES.map((c) => ({ value: c.value, label: t(c.labelKey) }))}
        />
        {!messageId && (
          <Switch
            label={t('lounge.createThread.privateLabel')}
            description={t('lounge.createThread.privateHelp')}
            checked={isPrivate}
            onChange={(e) => setIsPrivate(e.target.checked)}
          />
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.createThread.cancel')}
          </Button>
          <Button type="submit" loading={busy}>
            {t('lounge.createThread.title')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
