import { Avatar, Button, Checkbox, EmptyState, Modal, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { usePresenceMap } from '../data';
import { useOpenDm } from './dm';
import { friendsOf, useSocial, wireName } from './social';

/** A group DM holds you plus up to nine others. */
export const MAX_GROUP_RECIPIENTS = 9;

/** Create DM / Group DM picker (LNG-HOME-005): search friends, tick up to nine, create. */
export function CreateDmDialog({ onClose }: { onClose: () => void }) {
  const rows = useSocial((s) => s.relationships);
  const status = useSocial((s) => s.status);
  const load = useSocial((s) => s.loadRelationships);
  const presence = usePresenceMap();
  const openDm = useOpenDm();
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const friends = friendsOf(rows);
  const q = query.trim().toLowerCase();
  const shown = friends.filter(
    (r) =>
      !q || wireName(r.user).toLowerCase().includes(q) || r.user.username.toLowerCase().includes(q),
  );
  const full = picked.length >= MAX_GROUP_RECIPIENTS;

  const toggle = (id: string, on: boolean) =>
    setPicked((p) => (on ? (p.includes(id) ? p : [...p, id]) : p.filter((x) => x !== id)));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!picked.length) return;
    setBusy(true);
    setError(null);
    try {
      await openDm(picked);
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError && err.message
          ? err.message
          : 'Could not start that conversation. Try again.',
      );
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title="Select Friends"
      description={`You can add ${MAX_GROUP_RECIPIENTS - picked.length} more friend${
        MAX_GROUP_RECIPIENTS - picked.length === 1 ? '' : 's'
      }.`}
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Search friends"
          value={query}
          placeholder="Type the username of a friend"
          autoFocus
          onChange={(e) => setQuery(e.target.value)}
        />
        {status === 'loading' && friends.length === 0 && (
          <p role="status">
            <Spinner label="Loading friends" />
          </p>
        )}
        {status === 'error' && (
          <p role="alert" className="lng-form-error">
            Could not load your friends.
          </p>
        )}
        {status === 'ready' && friends.length === 0 && (
          <EmptyState
            title="You have no friends to add yet"
            description="Add friends from the Friends page first."
          />
        )}
        {friends.length > 0 && shown.length === 0 && (
          <p className="lng-muted">No friends match that search.</p>
        )}
        <ul className="lng-pick-list" aria-label="Friends">
          {shown.map((r) => {
            const on = picked.includes(r.id);
            return (
              <li key={r.id} className="lng-pick-row">
                <Avatar
                  name={wireName(r.user)}
                  size={32}
                  status={presence[r.id]?.status ?? 'offline'}
                />
                <Checkbox
                  label={wireName(r.user)}
                  description={r.user.username}
                  checked={on}
                  disabled={!on && full}
                  onChange={(e) => toggle(r.id, e.target.checked)}
                />
              </li>
            );
          })}
        </ul>
        {error && (
          <p role="alert" className="lng-form-error">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" loading={busy} disabled={!picked.length}>
            {picked.length > 1 ? 'Create Group DM' : 'Create DM'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
