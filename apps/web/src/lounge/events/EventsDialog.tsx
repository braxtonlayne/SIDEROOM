import { Badge, Button, EmptyState, Modal } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { formatDateTime, useT } from '../../i18n';
import { useChannels, useCurrentUser, useGuild } from '../data';
import { useLoungeUi } from '../store';
import {
  deleteEvent,
  EVENT_ENTITY,
  EVENT_STATUS,
  modifyEvent,
  setInterested,
  type ScheduledEvent,
} from './api';
import { useGuildEvents } from './useEvents';
import './events.css';

/**
 * The Events list (LNG-CHN-011): what is on and what is coming, with Interested, and for the creator
 * and people with Manage Events: Start, End, Cancel and Delete.
 */
export function EventsDialog({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const me = useCurrentUser();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const { events, error, reload } = useGuildEvents(guildId);
  const [busy, setBusy] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const p = guild?.permissions;
  const canCreate = !p || p.createEvents !== false || p.manageEvents === true;

  const run = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    setFailure(null);
    try {
      await fn();
      await reload();
    } catch (e) {
      setFailure(e instanceof ApiError && e.message ? e.message : t('lounge.events.failed'));
    } finally {
      setBusy(null);
    }
  };

  const where = (e: ScheduledEvent) =>
    e.entity_type === EVENT_ENTITY.external
      ? (e.entity_metadata?.location ?? '')
      : (channels.find((c) => c.id === e.channel_id)?.name ?? t('lounge.events.unknownChannel'));

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('lounge.events.title')}
      description={guild ? t('lounge.events.description', { name: guild.name }) : undefined}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.events.close')}
          </Button>
          {canCreate && (
            <Button onClick={() => openDialog({ kind: 'createEvent', guildId })}>
              {t('lounge.events.create')}
            </Button>
          )}
        </>
      }
    >
      {failure && (
        <p role="alert" className="lng-failed">
          {failure}
        </p>
      )}
      {error && !events && (
        <p role="alert" className="lng-failed">
          {t('lounge.events.loadFailed')}{' '}
          <Button size="sm" variant="secondary" onClick={() => void reload()}>
            {t('lounge.events.retry')}
          </Button>
        </p>
      )}
      {events === null && !error && <p className="lng-muted">{t('lounge.events.loading')}</p>}
      {events && events.length === 0 && (
        <EmptyState
          title={t('lounge.events.emptyTitle')}
          description={t('lounge.events.emptyDescription')}
        />
      )}
      {events && events.length > 0 && (
        <ul className="lng-events" aria-label={t('lounge.events.listLabel')}>
          {events.map((e) => {
            const mine = e.creator_id === me.id;
            const manage = mine || p?.manageEvents === true;
            const live = e.status === EVENT_STATUS.active;
            return (
              <li key={e.id} className="lng-event" data-live={live || undefined}>
                <div className="lng-event-main">
                  <div className="lng-event-when">
                    {live ? (
                      <Badge variant="success">{t('lounge.events.live')}</Badge>
                    ) : (
                      formatDateTime(new Date(e.scheduled_start_time), {
                        dateStyle: 'medium',
                        timeStyle: 'short',
                      })
                    )}
                  </div>
                  <h3 className="lng-event-name">{e.name}</h3>
                  <div className="lng-muted">{where(e)}</div>
                  {e.description && <p className="lng-event-desc">{e.description}</p>}
                  <div className="lng-muted">
                    {t('lounge.events.interested', { count: e.user_count ?? 0 })}
                  </div>
                </div>
                <div className="lng-event-actions">
                  <Button
                    size="sm"
                    variant={e.user_rsvp ? 'primary' : 'secondary'}
                    aria-pressed={!!e.user_rsvp}
                    disabled={busy === e.id}
                    onClick={() => void run(e.id, () => setInterested(guildId, e.id, !e.user_rsvp))}
                  >
                    {e.user_rsvp
                      ? t('lounge.events.interestedOn')
                      : t('lounge.events.interestedOff')}
                  </Button>
                  {manage && e.status === EVENT_STATUS.scheduled && (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy === e.id}
                      onClick={() =>
                        void run(e.id, () =>
                          modifyEvent(guildId, e.id, { status: EVENT_STATUS.active }),
                        )
                      }
                    >
                      {t('lounge.events.start')}
                    </Button>
                  )}
                  {manage && live && (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy === e.id}
                      onClick={() =>
                        void run(e.id, () =>
                          modifyEvent(guildId, e.id, { status: EVENT_STATUS.completed }),
                        )
                      }
                    >
                      {t('lounge.events.end')}
                    </Button>
                  )}
                  {manage && e.status === EVENT_STATUS.scheduled && (
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy === e.id}
                      onClick={() =>
                        void run(e.id, () =>
                          modifyEvent(guildId, e.id, { status: EVENT_STATUS.canceled }),
                        )
                      }
                    >
                      {t('lounge.events.cancel')}
                    </Button>
                  )}
                  {manage && (
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={t('lounge.events.deleteNamed', { name: e.name })}
                      disabled={busy === e.id}
                      onClick={() => void run(e.id, () => deleteEvent(guildId, e.id))}
                    >
                      {t('lounge.events.delete')}
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </Modal>
  );
}
