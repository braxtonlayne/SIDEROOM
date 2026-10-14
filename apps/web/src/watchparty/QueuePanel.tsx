// Queue tab (WP-FE-004): the upcoming titles with drag and button reordering, remove, skip, and "add from
// search". Who may do what follows the party's queue policy; the server decides, the UI mirrors it.
import { Button, Icon } from '@sideroom/ui';
import { useState } from 'react';
import { httpPartyApi } from './api';
import { partyController, useParty } from './runtime';
import { TitlePicker, type PickedTitle } from './TitlePicker';

export function QueuePanel() {
  const c = partyController();
  const party = useParty((s) => s.party);
  const [adding, setAdding] = useState(false);
  const [dragId, setDragId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  if (!party) return null;
  const you = party.you;
  const canQueue = !!you?.can_queue;
  const canOrder = !!you?.can_control;

  const run = async (fn: () => Promise<Parameters<typeof c.adoptRest>[0]>) => {
    setError(null);
    try {
      c.adoptRest(await fn());
    } catch {
      setError('That did not work. You may not have permission.');
    }
  };
  const move = (id: string, to: number) => run(() => httpPartyApi.queueMove(party.id, id, to));
  const add = async (t: PickedTitle) => {
    await run(() =>
      httpPartyApi.queueAdd(party.id, { title_id: t.titleId, episode_id: t.episodeId }),
    );
    setAdding(false);
  };

  return (
    <div className="wp-queue">
      <h3 className="wp-h">Now playing</h3>
      <p className="wp-now">
        {party.media.title}
        {party.media.episode_label ? (
          <span className="wp-muted"> · {party.media.episode_label}</span>
        ) : null}
      </p>

      <h3 className="wp-h">Up next</h3>
      {party.queue.length === 0 && <p className="wp-muted">Nothing queued.</p>}
      <ol className="wp-list" aria-label="Queue">
        {party.queue.map((q, i) => (
          <li
            key={q.id}
            className="wp-row"
            draggable={canOrder}
            data-dragging={dragId === q.id || undefined}
            onDragStart={() => setDragId(q.id)}
            onDragEnd={() => setDragId(null)}
            onDragOver={(e) => canOrder && e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              if (dragId && dragId !== q.id) void move(dragId, i);
              setDragId(null);
            }}
          >
            <span className="wp-row-main">
              <span className="wp-row-title">{q.title}</span>
              {q.episode_label && <span className="wp-muted">{q.episode_label}</span>}
            </span>
            {canOrder && (
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  iconOnly
                  aria-label={`Move ${q.title} up`}
                  disabled={i === 0}
                  onClick={() => void move(q.id, i - 1)}
                >
                  <span aria-hidden="true">↑</span>
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  iconOnly
                  aria-label={`Move ${q.title} down`}
                  disabled={i === party.queue.length - 1}
                  onClick={() => void move(q.id, i + 1)}
                >
                  <span aria-hidden="true">↓</span>
                </Button>
              </>
            )}
            {(canQueue || q.added_by === c.meId) && (
              <Button
                size="sm"
                variant="ghost"
                iconOnly
                aria-label={`Remove ${q.title} from the queue`}
                onClick={() => void run(() => httpPartyApi.queueRemove(party.id, q.id))}
              >
                <Icon name="close" size={16} />
              </Button>
            )}
          </li>
        ))}
      </ol>
      {error && (
        <p role="alert" className="wp-error">
          {error}
        </p>
      )}

      <div className="wp-queue-actions">
        {canOrder && party.queue.length > 0 && (
          <Button size="sm" variant="secondary" onClick={() => void c.skip()}>
            Skip to next
          </Button>
        )}
        {canQueue && !adding && (
          <Button size="sm" onClick={() => setAdding(true)}>
            Add to queue
          </Button>
        )}
      </div>
      {adding && (
        <div className="wp-add">
          <TitlePicker actionLabel="Add" onPick={(t) => void add(t)} />
          <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>
            Cancel
          </Button>
        </div>
      )}
      {!canQueue && <p className="wp-muted">Only the host and controllers can add titles.</p>}
    </div>
  );
}
