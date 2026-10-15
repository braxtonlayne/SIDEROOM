// Ticket threads (FTR-TICK-001): a member opens a private thread with the staff role; staff see every ticket and
// close them. The thread itself opens as an ordinary channel.
import { Button, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { formatDateTime, useT } from '../../i18n';
import { useCurrentUser, useGuild, useUsers } from '../data';
import { closeTicket, listTickets, openTicket, type Ticket } from '../features/api';
import { useLoungeGoto } from '../nav';
import { useLoad } from './useLoad';

export const MAX_SUBJECT = 100;

export function Tickets({ guildId, onNavigate }: { guildId: string; onNavigate: () => void }) {
  const t = useT();
  const me = useCurrentUser();
  const users = useUsers();
  const goto = useLoungeGoto();
  const staff = !!useGuild(guildId)?.permissions.manageGuild;
  const list = useLoad(() => listTickets(guildId), `tickets-${guildId}`);
  const [subject, setSubject] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const open = async () => {
    const s = subject.trim();
    if (!s) return;
    setBusy(true);
    setError(null);
    try {
      const ticket = await openTicket(guildId, s);
      setSubject('');
      goto(guildId, ticket.thread_id);
      onNavigate();
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.community.tickets.openFailed'),
      );
    } finally {
      setBusy(false);
    }
  };

  const close = async (tk: Ticket) => {
    setBusy(true);
    setError(null);
    try {
      await closeTicket(guildId, tk.id);
      list.reload();
    } catch {
      setError(t('lounge.community.tickets.closeFailed'));
    } finally {
      setBusy(false);
    }
  };

  const rows = list.data;
  return (
    <div className="lng-comm-stack">
      <form
        className="lng-comm-stack"
        onSubmit={(e) => {
          e.preventDefault();
          void open();
        }}
      >
        <TextInput
          label={t('lounge.community.tickets.subject')}
          hint={t('lounge.community.tickets.subjectHint')}
          value={subject}
          maxLength={MAX_SUBJECT}
          onChange={(e) => setSubject(e.target.value)}
        />
        <div>
          <Button type="submit" size="sm" disabled={busy || !subject.trim()}>
            {t('lounge.community.tickets.open')}
          </Button>
        </div>
      </form>
      {error && (
        <p className="lng-failed" role="alert">
          {error}
        </p>
      )}
      <h3 className="lng-comm-h">
        {staff
          ? t('lounge.community.tickets.allTickets')
          : t('lounge.community.tickets.yourTickets')}
      </h3>
      {list.error ? (
        <p className="lng-failed" role="alert">
          {t('lounge.community.tickets.loadFailed')}
        </p>
      ) : list.loading && !rows ? (
        <p className="lng-muted" role="status">
          {t('lounge.community.loading')}
        </p>
      ) : rows && rows.length === 0 ? (
        <p className="lng-muted">{t('lounge.community.tickets.empty')}</p>
      ) : (
        <ul className="lng-saved-list" aria-label={t('lounge.community.tickets.listLabel')}>
          {(rows ?? []).map((tk) => (
            <li key={tk.id} className="lng-saved-item">
              <div className="lng-saved-head">
                <strong>{tk.subject}</strong>
                <span className="lng-muted">{formatDateTime(new Date(tk.created_at))}</span>
              </div>
              <p className="lng-muted">
                {tk.status === 'open'
                  ? t('lounge.community.tickets.statusOpen')
                  : t('lounge.community.tickets.statusClosed')}
                {staff && tk.opener_id !== me.id
                  ? ` · ${t('lounge.community.tickets.openedBy', {
                      name:
                        users[tk.opener_id]?.displayName ??
                        t('lounge.community.suggestions.someone'),
                    })}`
                  : ''}
              </p>
              <div className="lng-saved-actions">
                {tk.status === 'open' && (
                  <button
                    type="button"
                    className="lng-link-btn"
                    onClick={() => {
                      goto(guildId, tk.thread_id);
                      onNavigate();
                    }}
                  >
                    {t('lounge.community.tickets.openThread')}
                  </button>
                )}
                {tk.status === 'open' && (staff || tk.opener_id === me.id) && (
                  <button
                    type="button"
                    className="lng-link-btn lng-danger"
                    disabled={busy}
                    onClick={() => void close(tk)}
                  >
                    {t('lounge.community.tickets.close')}
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
