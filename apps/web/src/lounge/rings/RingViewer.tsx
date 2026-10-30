// Plays one person's live rings, oldest first. The author also gets the viewer list and a delete button; the list is
// fetched only for the author's own ring (anybody else would get 404) and is never rendered for anyone else.
import { Button, Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { formatTime, useT } from '../../i18n';
import { ReportButton } from '../../moderation/reports';
import { useUsers } from '../data';
import {
  deleteRing,
  getRingViewers,
  markRingViewed,
  ringMediaUrl,
  type Ring,
  type RingFeedEntry,
  type RingViewerRow,
} from './api';
import { refreshRings } from './store';
import './rings.css';

function RingBody({ ring, name }: { ring: Ring; name: string }) {
  const t = useT();
  return (
    <>
      {ring.kind === 'photo' && (
        <img
          className="rng-media"
          src={ringMediaUrl(ring.id)}
          alt={t('lounge.rings.viewer.photoAlt', { name })}
        />
      )}
      {ring.kind === 'clip' && (
        <video
          className="rng-media"
          src={ringMediaUrl(ring.id)}
          controls
          playsInline
          aria-label={t('lounge.rings.viewer.clipLabel', { name })}
        />
      )}
      {ring.kind === 'text' && <p className="rng-text">{ring.body}</p>}
      {ring.kind === 'track' && ring.track && (
        <div className="rng-track">
          <div className="rng-meta">{t('lounge.rings.viewer.listening')}</div>
          <div className="rng-track-title">{ring.track.title}</div>
          {ring.track.artist && <div>{ring.track.artist}</div>}
        </div>
      )}
      {(ring.kind === 'photo' || ring.kind === 'clip') && ring.body && (
        <p className="rng-text">{ring.body}</p>
      )}
    </>
  );
}

function ViewerList({ ringId }: { ringId: string }) {
  const t = useT();
  const users = useUsers();
  const [rows, setRows] = useState<RingViewerRow[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    setRows(null);
    setFailed(false);
    getRingViewers(ringId)
      .then((r) => live && setRows(r))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [ringId]);
  if (failed) return <p className="rng-error">{t('lounge.rings.viewer.viewersFailed')}</p>;
  if (rows === null) return null;
  return (
    <section aria-label={t('lounge.rings.viewer.viewersTitle')}>
      <p>{t('lounge.rings.viewer.seenBy', { count: rows.length })}</p>
      {rows.length > 0 && (
        <ul className="rng-viewers">
          {rows.map((r) => (
            <li key={r.user_id}>
              {users[r.user_id]?.displayName ?? t('lounge.rings.viewer.unknownPerson')}
            </li>
          ))}
        </ul>
      )}
      <p className="rng-meta">{t('lounge.rings.viewer.viewersPrivate')}</p>
    </section>
  );
}

export function RingViewer({
  entry,
  name,
  onClose,
}: {
  entry: RingFeedEntry;
  name: string;
  onClose: () => void;
}) {
  const t = useT();
  const first = entry.rings.findIndex((r) => !r.seen);
  const [index, setIndex] = useState(first < 0 ? 0 : first);
  const [error, setError] = useState(false);
  const rings = entry.rings;
  const ring = rings[Math.min(index, rings.length - 1)];

  useEffect(() => {
    if (ring && !ring.mine) void markRingViewed(ring.id).catch(() => undefined);
  }, [ring?.id, ring?.mine]); // eslint-disable-line react-hooks/exhaustive-deps

  const close = () => {
    void refreshRings();
    onClose();
  };

  if (!ring) return null;

  const remove = () => {
    setError(false);
    deleteRing(ring.id)
      .then(() => refreshRings())
      .then(() => {
        if (rings.length <= 1) onClose();
        else setIndex((i) => Math.max(0, Math.min(i, rings.length - 2)));
      })
      .catch(() => setError(true));
  };

  return (
    <Modal
      open
      onClose={close}
      size="md"
      title={ring.mine ? t('lounge.rings.viewer.you') : t('lounge.rings.viewer.title', { name })}
    >
      <div className="rng-stage">
        <ol
          className="rng-progress"
          aria-label={t('lounge.rings.viewer.position', { n: index + 1, total: rings.length })}
        >
          {rings.map((r, i) => (
            <li key={r.id} data-done={i <= index || undefined} />
          ))}
        </ol>
        <RingBody ring={ring} name={name} />
        <div className="rng-meta">
          {t('lounge.rings.viewer.until', { time: formatTime(new Date(ring.expires_at)) })}
          {ring.mine && (
            <>
              {' · '}
              {ring.audience === 'friends'
                ? t('lounge.rings.viewer.audienceFriends')
                : t('lounge.rings.viewer.audienceGuilds', { count: ring.guild_ids.length })}
            </>
          )}
        </div>
        {ring.mine && <ViewerList ringId={ring.id} />}
        {error && (
          <p className="rng-error" role="alert">
            {t('lounge.rings.viewer.deleteFailed')}
          </p>
        )}
        <div className="rng-nav">
          <Button variant="secondary" disabled={index === 0} onClick={() => setIndex(index - 1)}>
            {t('lounge.rings.viewer.prev')}
          </Button>
          {ring.mine ? (
            <Button variant="danger" onClick={remove}>
              {t('lounge.rings.viewer.delete')}
            </Button>
          ) : (
            <ReportButton target={{ kind: 'status_ring', id: ring.id }} />
          )}
          <Button
            variant="secondary"
            disabled={index >= rings.length - 1}
            onClick={() => setIndex(index + 1)}
          >
            {t('lounge.rings.viewer.next')}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
