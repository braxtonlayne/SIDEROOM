import { Button, useToast } from '@sideroom/ui';
import { useNavigate, useSearch } from '@tanstack/react-router';
import { useEffect } from 'react';
import { useT } from '../../i18n';
import { onDispatch } from '../../store/events';
import { HOME } from '../channels';
import { useLoungeRoute } from '../nav';
import { useLoungeUi } from '../store';

/** The gateway dispatch for "an event you are interested in has started" (G-93). */
export const EVENT_START_DISPATCH = 'SR_SCHEDULED_EVENT_START';

interface StartPayload {
  guild_id?: string;
  guild_scheduled_event_id?: string;
  name?: string;
}

/**
 * Raises a toast when an event the member is interested in starts, and opens the Events dialog on
 * the event when the page was reached from the Web Push notification (`?event=<id>`). Renders nothing.
 */
export function EventAlerts() {
  const t = useT();
  const toast = useToast();
  const navigate = useNavigate();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const { guildId } = useLoungeRoute();
  const search = useSearch({ strict: false }) as { event?: unknown };
  const fromLink = typeof search.event === 'string' ? search.event : undefined;

  useEffect(() => {
    if (!fromLink || !guildId || guildId === HOME) return;
    openDialog({ kind: 'events', guildId, eventId: fromLink });
    // Clear the parameter so a reload does not reopen the dialog.
    void navigate({
      to: '.',
      search: ((prev: Record<string, unknown>) => {
        const rest = { ...prev };
        delete rest.event;
        return rest;
      }) as never,
      replace: true,
    });
  }, [fromLink, guildId, openDialog, navigate]);

  useEffect(
    () =>
      onDispatch((name, data) => {
        if (name !== EVENT_START_DISPATCH) return;
        const p = data as StartPayload | null;
        const guild = p?.guild_id;
        const id = p?.guild_scheduled_event_id;
        if (!guild || !id) return;
        toast.show({
          title: t('lounge.eventAlerts.started', { name: p?.name ?? '' }),
          description: (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => openDialog({ kind: 'events', guildId: guild, eventId: id })}
            >
              {t('lounge.eventAlerts.view')}
            </Button>
          ),
          variant: 'info',
          duration: 12000,
        });
      }),
    [toast, t, openDialog],
  );
  return null;
}
