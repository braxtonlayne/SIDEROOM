import { Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import type { Message } from '../data';

interface Reactor {
  id: string;
  username: string;
  global_name?: string | null;
}

const path = (m: Message, emoji: string) =>
  `/channels/${m.channelId}/messages/${m.id}/reactions/${encodeURIComponent(emoji)}`;

/** Who reacted (LNG-MSGUI-024): one tab per emoji, a user list, and Remove for moderators. */
export function ReactorsModal({
  message,
  initialEmoji,
  canManage,
  onClose,
}: {
  message: Message | null;
  initialEmoji?: string | undefined;
  canManage: boolean;
  onClose: () => void;
}) {
  const reactions = message?.reactions ?? [];
  const [emoji, setEmoji] = useState<string | undefined>(initialEmoji);
  const [users, setUsers] = useState<Reactor[] | null>(null);
  const [error, setError] = useState(false);
  const active = emoji ?? reactions[0]?.emoji;

  useEffect(() => setEmoji(initialEmoji), [initialEmoji, message?.id]);

  useEffect(() => {
    if (!message || !active) return;
    let live = true;
    setUsers(null);
    setError(false);
    apiFetch<Reactor[]>(`${path(message, active)}?limit=100`, { retries: 0 })
      .then((u) => live && setUsers(u))
      .catch(() => live && setError(true));
    return () => {
      live = false;
    };
  }, [message, active]);

  const remove = async (u: Reactor) => {
    if (!message || !active) return;
    try {
      await apiFetch<void>(`${path(message, active)}/${u.id}`, { method: 'DELETE', retries: 0 });
      setUsers((list) => list?.filter((x) => x.id !== u.id) ?? null);
    } catch {
      setError(true);
    }
  };

  return (
    <Modal open={!!message} onClose={onClose} title="Reactions" size="sm">
      <div role="tablist" aria-label="Reactions" className="lng-reactor-tabs">
        {reactions.map((r) => (
          <button
            key={r.emoji}
            type="button"
            role="tab"
            aria-selected={r.emoji === active}
            className="lng-reactor-tab"
            onClick={() => setEmoji(r.emoji)}
          >
            <span aria-hidden="true">{r.emoji}</span> {r.count}
            <span className="lng-sr-only"> people reacted with {r.emoji}</span>
          </button>
        ))}
      </div>
      <div role="tabpanel" aria-label={`People who reacted with ${active ?? ''}`}>
        {error && <p className="lng-failed">Could not load reactions.</p>}
        {!error && users === null && <p className="lng-muted">Loading…</p>}
        <ul className="lng-voter-list">
          {users?.map((u) => (
            <li key={u.id} className="lng-reactor-row">
              <span>{u.global_name || u.username}</span>
              {canManage && (
                <button
                  type="button"
                  className="lng-link-btn lng-danger"
                  aria-label={`Remove ${u.global_name || u.username}'s reaction`}
                  onClick={() => void remove(u)}
                >
                  Remove
                </button>
              )}
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
