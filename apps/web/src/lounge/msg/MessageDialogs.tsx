import { Button, ConfirmDialog, Modal } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { dmTitle } from '../channels';
import { useAllChannels, useDms, useUsers, type Message, type Snowflake } from '../data';

function Preview({ message }: { message: Message }) {
  const text =
    message.content.slice(0, 200) ||
    (message.attachments?.length ? `${message.attachments.length} attachment(s)` : '');
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
  return (
    <ConfirmDialog
      open={!!message}
      title="Delete Message"
      description={
        message ? (
          <>
            Are you sure you want to delete this message?
            <Preview message={message} />
          </>
        ) : undefined
      }
      confirmLabel="Delete"
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
  return (
    <ConfirmDialog
      open={!!message}
      title={pinned ? 'Unpin Message' : 'Pin It. Pin It Good.'}
      description={
        pinned
          ? 'Do you want to unpin this message from the channel?'
          : 'Do you want to pin this message to the channel? Anyone can then find it in the pinned messages.'
      }
      confirmLabel={pinned ? 'Unpin' : 'Pin'}
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
          out.push({ id: c.id, label: `#${c.name}`, group: 'Channels' });
      }
    }
    for (const d of dms) {
      out.push({
        id: d.id,
        label: dmTitle(d, (id) => users[id]?.displayName ?? 'Unknown'),
        group: 'Direct Messages',
      });
    }
    return out;
  }, [channelsByGuild, dms, users]);

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
      title="Forward To"
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button
            disabled={picked.length === 0}
            onClick={() => {
              onForward(picked, note.trim());
              close();
            }}
          >
            Send{picked.length > 0 ? ` (${picked.length})` : ''}
          </Button>
        </>
      }
    >
      {message && <Preview message={message} />}
      <input
        type="search"
        className="lng-emoji-search"
        aria-label="Search for channels or people"
        placeholder="Search for channels or people"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <ul className="lng-forward-list" aria-label="Destinations">
        {shown.length === 0 && <li className="lng-muted">No matches.</li>}
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
        aria-label="Add an optional message"
        placeholder="Add an optional message"
        value={note}
        maxLength={2000}
        onChange={(e) => setNote(e.target.value)}
      />
    </Modal>
  );
}
