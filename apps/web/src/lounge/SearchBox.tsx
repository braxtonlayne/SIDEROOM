import { useNavigate } from '@tanstack/react-router';
import { Icon } from '@sideroom/ui';
import { useEffect, useId, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { rest } from '../store';
import { HOME } from './channels';
import { useChannels, useUsers } from './data';
import { buildSearchQuery, highlightSegments } from './inviteFlow';
import { formatMessageTime } from './time';

type Status =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'done'; total: number; hits: rest.SearchHit[] };

const CHIPS = ['from:', 'in:', 'has:link', 'has:image', 'pinned:true'] as const;

/**
 * The header search box (LNG-SRC-003): runs `GET /guilds/{id}/messages/search` (or the channel route in a
 * DM), shows highlighted snippets and jumps to the picked message. `from:name` and `in:#channel` are
 * resolved to ids before the request because the server parser takes ids.
 */
export function SearchBox({
  guildId,
  channelId,
  label,
}: {
  guildId: string | undefined;
  channelId: string;
  label: string;
}) {
  const isDm = !guildId || guildId === HOME;
  const navigate = useNavigate();
  const users = useUsers();
  const channels = useChannels(isDm ? undefined : guildId);
  const [text, setText] = useState('');
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<Status>({ kind: 'idle' });
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const panelId = useId();
  // Drop answers that arrive after a newer search started.
  const seq = useRef(0);

  const lookups = useMemo(
    () => ({
      users: Object.values(users).map((u) => ({
        id: u.id,
        names: [u.username, u.displayName].filter(Boolean),
      })),
      channels: channels.map((c) => ({ id: c.id, name: c.name })),
    }),
    [users, channels],
  );

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', away);
    return () => document.removeEventListener('mousedown', away);
  }, [open]);

  const run = async (e?: FormEvent) => {
    e?.preventDefault();
    if (!text.trim()) return setStatus({ kind: 'idle' });
    const built = buildSearchQuery(text, lookups);
    setOpen(true);
    if ('error' in built) return setStatus({ kind: 'error', message: built.error });
    const mine = ++seq.current;
    setStatus({ kind: 'loading' });
    try {
      const res = isDm
        ? await rest.searchChannelMessages(channelId, built.q)
        : await rest.searchGuildMessages(guildId, built.q);
      if (mine !== seq.current) return;
      setStatus({ kind: 'done', total: res.total_results, hits: res.messages.map((g) => g[0]!) });
    } catch (err) {
      if (mine !== seq.current) return;
      setStatus({
        kind: 'error',
        message:
          err instanceof ApiError && err.status === 400
            ? 'That search could not be understood. Check your filters.'
            : 'Search failed. Try again.',
      });
    }
  };

  const addChip = (chip: string) => {
    setText((t) => (t && !t.endsWith(' ') ? `${t} ${chip}` : `${t}${chip}`));
    input.current?.focus();
  };

  const jump = (m: rest.SearchHit) => {
    setOpen(false);
    void navigate({
      to: '/channels/$guildId/$channelId/$messageId',
      params: { guildId: isDm ? HOME : guildId, channelId: m.channel_id, messageId: m.id },
    });
  };

  const channelName = (id: string) => channels.find((c) => c.id === id)?.name;

  return (
    <div className="lng-search-wrap" ref={root}>
      <form className="lng-search" role="search" onSubmit={(e) => void run(e)}>
        <Icon name="search" size={16} />
        <input
          ref={input}
          type="search"
          placeholder="Search"
          aria-label={label}
          aria-controls={open ? panelId : undefined}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onFocus={() => status.kind !== 'idle' && setOpen(true)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') setOpen(false);
          }}
        />
      </form>
      {open && (
        <section className="lng-search-panel" id={panelId} aria-label="Search results">
          <div className="lng-search-chips" role="group" aria-label="Search filters">
            {CHIPS.filter((c) => !isDm || c !== 'in:').map((c) => (
              <button type="button" key={c} className="lng-chip" onClick={() => addChip(c)}>
                {c}
              </button>
            ))}
          </div>
          {status.kind === 'loading' && <p role="status">Searching…</p>}
          {status.kind === 'error' && <p role="alert">{status.message}</p>}
          {status.kind === 'done' && (
            <>
              <h2 className="lng-search-count">
                {status.total === 0
                  ? 'No results'
                  : `${status.total.toLocaleString()} result${status.total === 1 ? '' : 's'}`}
              </h2>
              <ul className="lng-search-results">
                {status.hits.map((m) => {
                  const snippet = m.sr_snippet ?? { text: m.content, highlights: [] };
                  const name = users[m.author.id]?.displayName ?? m.author.username ?? 'Unknown';
                  const where = isDm ? undefined : channelName(m.channel_id);
                  return (
                    <li key={m.id}>
                      <button type="button" className="lng-search-hit" onClick={() => jump(m)}>
                        <span className="lng-search-meta">
                          {where && <span>#{where}</span>}
                          <strong>{name}</strong>
                          <time dateTime={m.timestamp}>{formatMessageTime(m.timestamp)}</time>
                        </span>
                        <span className="lng-search-snippet">
                          {highlightSegments(snippet.text, snippet.highlights).map((s, i) =>
                            s.hit ? <mark key={i}>{s.text}</mark> : <span key={i}>{s.text}</span>,
                          )}
                        </span>
                        <span className="lng-search-jump">Jump</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </>
          )}
        </section>
      )}
    </div>
  );
}
