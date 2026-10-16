// Queue tab (WP-FE-004): the upcoming titles with drag and button reordering, remove, skip, and "add from
// search". Who may do what follows the party's queue policy; the server decides, the UI mirrors it.
import { Button, Icon } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { httpPartyApi } from './api';
import { partyController, useParty } from './runtime';
import { TitlePicker, type PickedTitle } from './TitlePicker';

export function QueuePanel() {
  const tr = useT();
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
      setError(tr('watchparty.queue.failed'));
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
      <h3 className="wp-h">{tr('watchparty.queue.nowPlaying')}</h3>
      <p className="wp-now">
        {party.media ? party.media.title : tr('watchparty.empty.nothingYet')}
        {party.media?.episode_label ? (
          <span className="wp-muted"> · {party.media.episode_label}</span>
        ) : null}
      </p>

      <h3 className="wp-h">{tr('watchparty.queue.upNext')}</h3>
      {party.queue.length === 0 && <p className="wp-muted">{tr('watchparty.queue.empty')}</p>}
      <ol className="wp-list" aria-label={tr('watchparty.queue.label')}>
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
                  aria-label={tr('watchparty.queue.moveUp', { title: q.title })}
                  disabled={i === 0}
                  onClick={() => void move(q.id, i - 1)}
                >
                  <span aria-hidden="true">↑</span>
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  iconOnly
                  aria-label={tr('watchparty.queue.moveDown', { title: q.title })}
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
                aria-label={tr('watchparty.queue.remove', { title: q.title })}
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
            {tr('watchparty.queue.skip')}
          </Button>
        )}
        {canQueue && !adding && (
          <Button size="sm" onClick={() => setAdding(true)}>
            {tr('watchparty.queue.add')}
          </Button>
        )}
      </div>
      {adding && (
        <div className="wp-add">
          <TitlePicker actionLabel={tr('watchparty.queue.addAction')} onPick={(t) => void add(t)} />
          <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>
            {tr('watchparty.queue.cancel')}
          </Button>
        </div>
      )}
      {!canQueue && <p className="wp-muted">{tr('watchparty.queue.onlyHost')}</p>}
    </div>
  );
}
