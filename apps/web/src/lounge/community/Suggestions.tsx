// Suggestions (FTR-SUGG-001): members post an idea to the server's suggestions channel; staff set a status and a
// response. Vote counts are read from the reactions on the suggestion card (the server seeds the up and down
// emoji first, in that order, and counts its own seed as one vote).
import { Button, Select, TextArea } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { formatDateTime, useT, type MessageKey } from '../../i18n';
import { useGuild, useUsers } from '../data';
import {
  listSuggestions,
  respondToSuggestion,
  submitSuggestion,
  type Suggestion,
  type SuggestionStatus,
} from '../features/api';
import { useLoad } from './useLoad';

export const STATUSES: SuggestionStatus[] = [
  'open',
  'considering',
  'approved',
  'denied',
  'implemented',
];
const STATUS_KEYS: Record<SuggestionStatus, MessageKey> = {
  open: 'lounge.community.suggestions.statusOpen',
  considering: 'lounge.community.suggestions.statusConsidering',
  approved: 'lounge.community.suggestions.statusApproved',
  denied: 'lounge.community.suggestions.statusDenied',
  implemented: 'lounge.community.suggestions.statusImplemented',
};
export const MAX_SUGGESTION = 2000;
const MAX_RESPONSE = 1000;

export interface Votes {
  up: number;
  down: number;
}

/** Votes from a card's reactions: the first two are the seeded up and down emoji, each carrying the seed's own vote. */
export function votesOf(reactions: { count: number }[] | undefined): Votes {
  const n = (i: number) => Math.max((reactions?.[i]?.count ?? 1) - 1, 0);
  return { up: n(0), down: n(1) };
}

async function fetchVotes(s: Suggestion): Promise<Votes | null> {
  try {
    const m = await apiFetch<{ reactions?: { count: number }[] }>(
      `/channels/${encodeURIComponent(s.channel_id)}/messages/${encodeURIComponent(s.message_id)}`,
      { retries: 0 },
    );
    return votesOf(m.reactions);
  } catch {
    return null;
  }
}

export function Suggestions({ guildId }: { guildId: string }) {
  const t = useT();
  const guild = useGuild(guildId);
  const staff = !!guild?.permissions.manageGuild;
  const [filter, setFilter] = useState<SuggestionStatus | ''>('');
  const list = useLoad(
    () => listSuggestions(guildId, filter || undefined),
    `sugg-${guildId}-${filter}`,
  );
  const [votes, setVotes] = useState<Record<string, Votes | null>>({});
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const rows = list.data;
  useEffect(() => {
    if (!rows) return;
    let live = true;
    void Promise.all(rows.slice(0, 25).map(async (s) => [s.id, await fetchVotes(s)] as const)).then(
      (pairs) => {
        if (live) setVotes(Object.fromEntries(pairs));
      },
    );
    return () => {
      live = false;
    };
  }, [rows]);

  const submit = async () => {
    const content = text.trim();
    if (!content) return;
    setBusy(true);
    setError(null);
    try {
      await submitSuggestion(guildId, content);
      setText('');
      list.reload();
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.community.suggestions.failed'),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="lng-comm-stack">
      <form
        className="lng-comm-stack"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <TextArea
          label={t('lounge.community.suggestions.compose')}
          hint={t('lounge.community.suggestions.composeHint')}
          value={text}
          maxLength={MAX_SUGGESTION}
          rows={3}
          onChange={(e) => setText(e.target.value)}
        />
        <div>
          <Button type="submit" size="sm" disabled={busy || !text.trim()}>
            {t('lounge.community.suggestions.send')}
          </Button>
        </div>
        {error && (
          <p className="lng-failed" role="alert">
            {error}
          </p>
        )}
      </form>
      <Select
        aria-label={t('lounge.community.suggestions.filter')}
        options={[
          { value: '', label: t('lounge.community.suggestions.all') },
          ...STATUSES.map((s) => ({ value: s, label: t(STATUS_KEYS[s]) })),
        ]}
        value={filter}
        onValueChange={(v) => setFilter(v as SuggestionStatus | '')}
      />
      {list.error ? (
        <p className="lng-failed" role="alert">
          {t('lounge.community.suggestions.loadFailed')}
        </p>
      ) : list.loading && !rows ? (
        <p className="lng-muted" role="status">
          {t('lounge.community.loading')}
        </p>
      ) : rows && rows.length === 0 ? (
        <p className="lng-muted">{t('lounge.community.suggestions.empty')}</p>
      ) : (
        <ul className="lng-saved-list" aria-label={t('lounge.community.suggestions.listLabel')}>
          {(rows ?? []).map((s) => (
            <SuggestionRow
              key={s.id}
              s={s}
              guildId={guildId}
              staff={staff}
              votes={votes[s.id] ?? null}
              onChanged={list.reload}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function SuggestionRow({
  s,
  guildId,
  staff,
  votes,
  onChanged,
}: {
  s: Suggestion;
  guildId: string;
  staff: boolean;
  votes: Votes | null;
  onChanged: () => void;
}) {
  const t = useT();
  const users = useUsers();
  const [editing, setEditing] = useState(false);
  const [status, setStatus] = useState<SuggestionStatus>(s.status);
  const [response, setResponse] = useState(s.response);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const author = users[s.author_id]?.displayName ?? t('lounge.community.suggestions.someone');

  const save = async () => {
    setBusy(true);
    setError(false);
    try {
      await respondToSuggestion(guildId, s.id, status, response.trim());
      setEditing(false);
      onChanged();
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <li className="lng-saved-item">
      <div className="lng-saved-head">
        <strong>{author}</strong>
        <span className="lng-muted">{formatDateTime(new Date(s.created_at))}</span>
      </div>
      <p className="lng-saved-body">{s.content}</p>
      <p className="lng-muted">
        <span className="lng-status-chip" data-status={s.status}>
          {t(STATUS_KEYS[s.status])}
        </span>
        {votes && (
          <span>
            {' '}
            {t('lounge.community.suggestions.votes', { up: votes.up, down: votes.down })}
          </span>
        )}
      </p>
      {s.response && (
        <p className="lng-sugg-response">
          <strong>{t('lounge.community.suggestions.staffResponse')}</strong> {s.response}
        </p>
      )}
      {staff &&
        (editing ? (
          <div className="lng-comm-stack">
            <Select
              aria-label={t('lounge.community.suggestions.setStatus')}
              options={STATUSES.map((x) => ({ value: x, label: t(STATUS_KEYS[x]) }))}
              value={status}
              onValueChange={(v) => setStatus(v as SuggestionStatus)}
            />
            <TextArea
              label={t('lounge.community.suggestions.responseLabel')}
              value={response}
              maxLength={MAX_RESPONSE}
              rows={2}
              onChange={(e) => setResponse(e.target.value)}
            />
            <div>
              <Button size="sm" disabled={busy} onClick={() => void save()}>
                {t('lounge.community.suggestions.saveResponse')}
              </Button>{' '}
              <Button size="sm" variant="secondary" onClick={() => setEditing(false)}>
                {t('lounge.community.cancel')}
              </Button>
            </div>
            {error && (
              <p className="lng-failed" role="alert">
                {t('lounge.community.suggestions.failed')}
              </p>
            )}
          </div>
        ) : (
          <button type="button" className="lng-link-btn" onClick={() => setEditing(true)}>
            {t('lounge.community.suggestions.respond')}
          </button>
        ))}
    </li>
  );
}
