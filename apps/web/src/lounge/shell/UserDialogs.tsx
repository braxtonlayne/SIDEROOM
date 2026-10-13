import { Button, Modal, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useStore } from '../../store';
import { useMembers } from '../data';

const NOTE_MAX = 256;

/** Private note about a person (LNG-HOME-008, LNG-GLD-023): `GET/PUT /users/@me/notes/{id}`. */
export function NoteDialog({
  userId,
  name,
  onClose,
}: {
  userId: string;
  name: string;
  onClose: () => void;
}) {
  const [note, setNote] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    apiFetch<{ note: string }>(`/users/@me/notes/${encodeURIComponent(userId)}`, { retries: 0 })
      .then((n) => live && setNote(n.note))
      .catch(() => undefined) // 404 = no note yet
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [userId]);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await apiFetch<void>(`/users/@me/notes/${encodeURIComponent(userId)}`, {
        method: 'PUT',
        json: { note: note.trim() || null },
        retries: 0,
      });
      onClose();
    } catch {
      setError('Could not save the note. Try again.');
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={`Note for ${name}`}
      description="Only you can see this."
    >
      {loading ? (
        <p role="status">
          <Spinner label="Loading note" />
        </p>
      ) : (
        <form className="lng-form" onSubmit={(e) => void save(e)}>
          <TextInput
            label="Note"
            value={note}
            maxLength={NOTE_MAX}
            autoFocus
            error={error}
            onChange={(e) => setNote(e.target.value)}
          />
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" loading={busy}>
              Save
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}

/** Change a member's server nickname (LNG-GLD-023). The server checks Change/Manage Nicknames. */
export function NicknameDialog({
  guildId,
  userId,
  onClose,
}: {
  guildId: string;
  userId: string;
  onClose: () => void;
}) {
  const member = useMembers(guildId).find((m) => m.user.id === userId);
  const [nick, setNick] = useState(member?.nick ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const next = nick.trim() || null;
    try {
      await apiFetch<unknown>(`/guilds/${guildId}/members/${userId}`, {
        method: 'PATCH',
        json: { nick: next },
        retries: 0,
      });
      useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
        guild_id: guildId,
        user: { id: userId },
        nick: next,
        roles: member?.roleIds ?? [],
      });
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? 'You do not have permission to change this nickname.'
          : 'Could not change the nickname. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <Modal open onClose={onClose} size="sm" title="Change Nickname">
      <form className="lng-form" onSubmit={(e) => void save(e)}>
        <TextInput
          label="Nickname"
          value={nick}
          maxLength={32}
          placeholder={member?.user.displayName ?? ''}
          autoFocus
          error={error}
          onChange={(e) => setNick(e.target.value)}
        />
        <div className="lng-form-actions">
          {member?.nick && (
            <Button variant="ghost" onClick={() => setNick('')}>
              Reset Nickname
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Save
          </Button>
        </div>
      </form>
    </Modal>
  );
}
