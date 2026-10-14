import { useNavigate } from '@tanstack/react-router';
import { Icon } from '@sideroom/ui';
import { useEffect, useId, useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { formatNumber, useT } from '../i18n';
import { rest } from '../store';
import { HOME } from './channels';
import { useChannels, useUsers } from './data';
import { buildSearchQuery, highlightSegments } from './inviteFlow';
import {
  applyFilterOption,
  currentToken,
  filterOptions,
  groupByChannel,
  MAX_OFFSET,
  PAGE_SIZE,
  searchHistory,
  SORT_LABELS,
  sortParams,
  type SearchSort,
} from './searchUi';
import { ContextLine, useHitContext } from './searchContext';
import { formatMessageTime } from './time';

type Status =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'done'; total: number; hits: rest.SearchHit[] };

const CHIPS = ['from:', 'in:', 'has:link', 'has:image', 'pinned:true'] as const;

/** One result: the hit with the message before and after it (LNG-SRC-005) and a Jump button. */
function HitRow({
  hit,
  onJump,
  children,
}: {
  hit: rest.SearchHit;
  onJump: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const { ref, ctx } = useHitContext(hit.channel_id, hit.id);
  return (
    <li ref={ref} className="lng-search-row">
      <ContextLine n={ctx?.before} side="before" />
      <button type="button" className="lng-search-hit" onClick={onJump}>
        {children}
        <span className="lng-search-jump">{t('lounge.searchBox.jump')}</span>
      </button>
      <ContextLine n={ctx?.after} side="after" />
    </li>
  );
}

/**
 * The header search box (LNG-SRC-003/004/005/006): runs `GET /guilds/{id}/messages/search` (or the channel
 * route in a DM). Typing a filter opens an options list (people, channels, has:, dates); results are grouped
 * by channel with sort and paging, each with the message before and after it; recent queries are kept on the
 * device. `from:name` and `in:#channel` are resolved to ids before the request because the server parser
 * takes ids.
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
  const t = useT();
  const isDm = !guildId || guildId === HOME;
  const navigate = useNavigate();
  const users = useUsers();
  const channels = useChannels(isDm ? undefined : guildId);
  const [text, setText] = useState('');
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<Status>({ kind: 'idle' });
  const [sort, setSort] = useState<SearchSort>('new');
  const [page, setPage] = useState(0);
  const [history, setHistory] = useState<string[]>(() => searchHistory.list());
  const [optIndex, setOptIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const listId = useId();
  // Drop answers that arrive after a newer search started.
  const seq = useRef(0);
  // The text of the last submitted search, so sort and paging re-run it rather than half-typed input.
  const submitted = useRef('');

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

  const token = currentToken(text).token;
  const options = useMemo(
    () => (open ? filterOptions(token, lookups, { isDm }) : []),
    [open, token, lookups, isDm],
  );
  const optActive = Math.min(optIndex, Math.max(0, options.length - 1));

  const exec = async (raw: string, nextSort: SearchSort, nextPage: number) => {
    if (!raw.trim()) return setStatus({ kind: 'idle' });
    const built = buildSearchQuery(raw, lookups);
    setOpen(true);
    if ('error' in built) return setStatus({ kind: 'error', message: built.error });
    const mine = ++seq.current;
    setStatus({ kind: 'loading' });
    const opts = { offset: nextPage * PAGE_SIZE, extra: sortParams(nextSort) };
    try {
      const res = isDm
        ? await rest.searchChannelMessages(channelId, built.q, opts)
        : await rest.searchGuildMessages(guildId, built.q, opts);
      if (mine !== seq.current) return;
      setStatus({ kind: 'done', total: res.total_results, hits: res.messages.map((g) => g[0]!) });
      if (nextPage === 0) setHistory(searchHistory.add(raw));
    } catch (err) {
      if (mine !== seq.current) return;
      setStatus({
        kind: 'error',
        message:
          err instanceof ApiError && err.status === 400
            ? t('lounge.searchBox.badQuery')
            : t('lounge.searchBox.failed'),
      });
    }
  };

  const run = (e?: FormEvent, raw = text) => {
    e?.preventDefault();
    submitted.current = raw;
    setPage(0);
    void exec(raw, sort, 0);
  };

  const changeSort = (s: SearchSort) => {
    setSort(s);
    setPage(0);
    void exec(submitted.current || text, s, 0);
  };
  const goPage = (p: number) => {
    setPage(p);
    void exec(submitted.current || text, sort, p);
  };

  const addChip = (chip: string) => {
    setText((prev) => (prev && !prev.endsWith(' ') ? `${prev} ${chip}` : `${prev}${chip}`));
    input.current?.focus();
  };

  const pickOption = (i: number) => {
    const o = options[i];
    if (!o) return;
    setText((prev) => applyFilterOption(prev, o));
    setOptIndex(0);
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

  const pages =
    status.kind === 'done'
      ? Math.max(1, Math.min(Math.ceil(status.total / PAGE_SIZE), MAX_OFFSET / PAGE_SIZE + 1))
      : 1;

  const hasText = text.trim() !== '';
  const showHistory = open && !hasText && history.length > 0;

  return (
    <div className="lng-search-wrap" ref={root}>
      <form className="lng-search" role="search" onSubmit={(e) => run(e)}>
        <Icon name="search" size={16} />
        <input
          ref={input}
          type="search"
          placeholder={t('lounge.searchBox.placeholder')}
          aria-label={label}
          aria-controls={open ? panelId : undefined}
          aria-activedescendant={options.length ? `${listId}-${optActive}` : undefined}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setOptIndex(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') setOpen(false);
            else if (options.length && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
              e.preventDefault();
              const n = options.length;
              setOptIndex((optActive + (e.key === 'ArrowDown' ? 1 : n - 1)) % n);
            } else if (options.length && e.key === 'Tab' && !e.shiftKey) {
              e.preventDefault();
              pickOption(optActive);
            }
          }}
        />
      </form>
      {open && (
        <section
          className="lng-search-panel"
          id={panelId}
          aria-label={t('lounge.searchBox.results')}
        >
          <div className="lng-search-chips" role="group" aria-label={t('lounge.searchBox.filters')}>
            {CHIPS.filter((c) => !isDm || c !== 'in:').map((c) => (
              <button type="button" key={c} className="lng-chip" onClick={() => addChip(c)}>
                {c}
              </button>
            ))}
          </div>
          {options.length > 0 && (
            <ul
              id={listId}
              role="listbox"
              aria-label={t('lounge.searchBox.suggestions')}
              className="lng-ac-list"
            >
              {options.map((o, i) => (
                <li
                  key={o.insert}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === optActive}
                  className="lng-ac-row"
                  data-active={i === optActive || undefined}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    pickOption(i);
                  }}
                >
                  <span className="lng-ac-label">{o.label}</span>
                  {o.hint && <span className="lng-ac-hint">{o.hint}</span>}
                </li>
              ))}
            </ul>
          )}
          {open && /^(before|during|after):/i.test(token) && (
            <label className="lng-search-date">
              <span>{t('lounge.searchBox.pickDate')}</span>
              <input
                type="date"
                aria-label={t('lounge.searchBox.pickDate')}
                onChange={(e) => {
                  if (!e.target.value) return;
                  const key = /^(w+):/.exec(token)![1];
                  setText(
                    (prev) => `${prev.slice(0, currentToken(prev).start)}${key}:${e.target.value} `,
                  );
                  input.current?.focus();
                }}
              />
            </label>
          )}
          {showHistory && (
            <div className="lng-search-history">
              <div className="lng-search-history-head">
                <h2 className="lng-search-count">{t('lounge.searchBox.recent')}</h2>
                <button
                  type="button"
                  className="lng-link-btn"
                  onClick={() => {
                    searchHistory.clear();
                    setHistory([]);
                  }}
                >
                  {t('lounge.searchBox.clearHistory')}
                </button>
              </div>
              <ul className="lng-search-results">
                {history.map((h) => (
                  <li key={h}>
                    <button
                      type="button"
                      className="lng-search-hit"
                      onClick={() => {
                        setText(h);
                        run(undefined, h);
                      }}
                    >
                      <Icon name="clock" size={16} /> {h}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {hasText && status.kind === 'loading' && (
            <p role="status">{t('lounge.searchBox.searching')}</p>
          )}
          {hasText && status.kind === 'error' && <p role="alert">{status.message}</p>}
          {hasText && status.kind === 'done' && (
            <>
              <div className="lng-search-bar">
                <h2 className="lng-search-count">
                  {status.total === 0
                    ? t('lounge.searchBox.noResults')
                    : t('lounge.searchBox.resultCount', {
                        count: status.total,
                        formatted: formatNumber(status.total),
                      })}
                </h2>
                <label className="lng-search-sort">
                  <span className="lng-sr-only">{t('lounge.searchBox.sortBy')}</span>
                  <select value={sort} onChange={(e) => changeSort(e.target.value as SearchSort)}>
                    {(Object.keys(SORT_LABELS) as SearchSort[]).map((s) => (
                      <option key={s} value={s}>
                        {SORT_LABELS[s]}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              {groupByChannel(status.hits).map((g) => {
                const where = isDm ? undefined : channelName(g.channelId);
                return (
                  <div key={g.channelId} className="lng-search-group">
                    {where && <h3 className="lng-search-channel">#{where}</h3>}
                    <ul className="lng-search-results">
                      {g.hits.map((m) => {
                        const snippet = m.sr_snippet ?? { text: m.content, highlights: [] };
                        const name =
                          users[m.author.id]?.displayName ??
                          m.author.username ??
                          t('lounge.searchBox.unknown');
                        return (
                          <HitRow key={m.id} hit={m} onJump={() => jump(m)}>
                            <span className="lng-search-meta">
                              <strong>{name}</strong>
                              <time dateTime={m.timestamp}>{formatMessageTime(m.timestamp)}</time>
                            </span>
                            <span className="lng-search-snippet">
                              {highlightSegments(snippet.text, snippet.highlights).map((s, i) =>
                                s.hit ? (
                                  <mark key={i}>{s.text}</mark>
                                ) : (
                                  <span key={i}>{s.text}</span>
                                ),
                              )}
                            </span>
                          </HitRow>
                        );
                      })}
                    </ul>
                  </div>
                );
              })}
              {pages > 1 && (
                <nav className="lng-search-pages" aria-label={t('lounge.searchBox.pages')}>
                  <button
                    type="button"
                    className="lng-link-btn"
                    disabled={page === 0}
                    onClick={() => goPage(page - 1)}
                  >
                    {t('lounge.searchBox.previous')}
                  </button>
                  <span aria-live="polite">
                    {t('lounge.searchBox.pageOf', { page: page + 1, pages })}
                  </span>
                  <button
                    type="button"
                    className="lng-link-btn"
                    disabled={page + 1 >= pages}
                    onClick={() => goPage(page + 1)}
                  >
                    {t('lounge.searchBox.next')}
                  </button>
                </nav>
              )}
            </>
          )}
        </section>
      )}
    </div>
  );
}
