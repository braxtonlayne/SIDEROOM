import { Button, ConfirmDialog, Modal } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { dmTitle } from '../channels';
import { useAllChannels, useDms, useUsers, type Message, type Snowflake } from '../data';

function Preview({ message }: { message: Message }) {
  const t = useT();
  const text =
    message.content.slice(0, 200) ||
    (message.attachments?.length
      ? t('lounge.messageDialogs.attachmentCount', { n: message.attachments.length })
      : '');
  return (
    <span className="lng-preview">
      <strong>{message.author.displayName}</strong>
      <span className="lng-preview-text">{text}</span>
    </span>
  );
}

/** Delete confirmation (LNG-MSGUI-028). Shift-clicking the delete button skips it (handled by the caller). */
export function DeleteMessageDialog({
  message,
  onConfirm,
  onCancel,
}: {
  message: Message | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  return (
    <ConfirmDialog
      open={!!message}
      title={t('lounge.messageDialogs.deleteTitle')}
      description={
        message ? (
          <>
            {t('lounge.messageDialogs.deleteConfirm')}
            <Preview message={message} />
          </>
        ) : undefined
      }
      confirmLabel={t('lounge.messageDialogs.delete')}
      danger
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

export function PinMessageDialog({
  message,
  pinned,
  onConfirm,
  onCancel,
}: {
  message: Message | null;
  pinned: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  return (
    <ConfirmDialog
      open={!!message}
      title={pinned ? t('lounge.messageDialogs.unpinTitle') : t('lounge.messageDialogs.pinTitle')}
      description={
        pinned ? t('lounge.messageDialogs.unpinConfirm') : t('lounge.messageDialogs.pinConfirm')
      }
      confirmLabel={pinned ? t('lounge.messageDialogs.unpin') : t('lounge.messageDialogs.pin')}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

export const MAX_FORWARD_TARGETS = 5;

interface Dest {
  id: Snowflake;
  label: string;
  group: string;
}

/** Forward modal (LNG-MSGUI-029): destination search, up to five picks, optional note. */
export function ForwardDialog({
  message,
  onForward,
  onClose,
}: {
  message: Message | null;
  onForward: (targets: Snowflake[], note: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const channelsByGuild = useAllChannels();
  const dms = useDms();
  const users = useUsers();
  const [q, setQ] = useState('');
  const [picked, setPicked] = useState<Snowflake[]>([]);
  const [note, setNote] = useState('');

  const dests = useMemo<Dest[]>(() => {
    const out: Dest[] = [];
    for (const list of Object.values(channelsByGuild)) {
      for (const c of list) {
        if ((c.type === 'text' || c.type === 'announcement') && c.canSend)
          out.push({
            id: c.id,
            label: `#${c.name}`,
            group: t('lounge.messageDialogs.groupChannels'),
          });
      }
    }
    for (const d of dms) {
      out.push({
        id: d.id,
        label: dmTitle(d, (id) => users[id]?.displayName ?? t('lounge.messageDialogs.unknownUser')),
        group: t('lounge.messageDialogs.groupDirectMessages'),
      });
    }
    return out;
  }, [channelsByGuild, dms, users, t]);

  const shown = dests.filter((d) => d.label.toLowerCase().includes(q.trim().toLowerCase()));
  const toggle = (id: Snowflake) =>
    setPicked((p) =>
      p.includes(id) ? p.filter((x) => x !== id) : p.length < MAX_FORWARD_TARGETS ? [...p, id] : p,
    );
  const close = () => {
    setQ('');
    setPicked([]);
    setNote('');
    onClose();
  };
  return (
    <Modal
      open={!!message}
      onClose={close}
      title={t('lounge.messageDialogs.forwardTo')}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t('lounge.messageDialogs.cancel')}
          </Button>
          <Button
            disabled={picked.length === 0}
            onClick={() => {
              onForward(picked, note.trim());
              close();
            }}
          >
            {picked.length > 0
              ? t('lounge.messageDialogs.sendCount', { n: picked.length })
              : t('lounge.messageDialogs.send')}
          </Button>
        </>
      }
    >
      {message && <Preview message={message} />}
      <input
        type="search"
        className="lng-emoji-search"
        aria-label={t('lounge.messageDialogs.searchTargets')}
        placeholder={t('lounge.messageDialogs.searchTargets')}
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <ul className="lng-forward-list" aria-label={t('lounge.messageDialogs.destinations')}>
        {shown.length === 0 && (
          <li className="lng-muted">{t('lounge.messageDialogs.noMatches')}</li>
        )}
        {shown.map((d) => {
          const on = picked.includes(d.id);
          return (
            <li key={d.id}>
              <label className="lng-forward-row">
                <input
                  type="checkbox"
                  checked={on}
                  disabled={!on && picked.length >= MAX_FORWARD_TARGETS}
                  onChange={() => toggle(d.id)}
                />
                <span>{d.label}</span>
                <span className="lng-muted">{d.group}</span>
              </label>
            </li>
          );
        })}
      </ul>
      <input
        type="text"
        className="lng-emoji-search"
        aria-label={t('lounge.messageDialogs.optionalMessage')}
        placeholder={t('lounge.messageDialogs.optionalMessage')}
        value={note}
        maxLength={2000}
        onChange={(e) => setNote(e.target.value)}
      />
    </Modal>
  );
}
