// LIB-UI-003: instant search with a grouped results dropdown (series, authors, genres, tags) and keyboard
// navigation. Combobox pattern: focus stays in the input, arrows move the active option, Enter opens it.
import { Icon } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { keys, searchLibrary } from './api';
import { AgeBadge, label } from './ui';

type Kind = 'series' | 'author' | 'genre' | 'tag';
interface Option {
  id: string;
  kind: Kind;
  text: string;
  /** Series id for series options. */
  seriesId?: string;
  restricted?: boolean;
}

const GROUPS: { kind: Kind; title: string }[] = [
  { kind: 'series', title: 'Series' },
  { kind: 'author', title: 'Authors' },
  { kind: 'genre', title: 'Genres' },
  { kind: 'tag', title: 'Tags' },
];

export const SEARCH_DEBOUNCE_MS = 150;

export function SearchBox() {
  const navigate = useNavigate();
  const uid = useId();
  const listId = `${uid}-list`;
  const [text, setText] = useState('');
  const [debounced, setDebounced] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const wrap = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(text.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [text]);

  const enabled = debounced.length >= 2;
  const result = useQuery({
    queryKey: keys.search(debounced),
    queryFn: ({ signal }) => searchLibrary(debounced, 8, signal),
    enabled,
    staleTime: 30_000,
  });

  const options = useMemo<Option[]>(() => {
    const d = result.data;
    if (!d || !enabled) return [];
    return [
      ...d.series.slice(0, 5).map((s) => ({
        id: `s-${s.id}`,
        kind: 'series' as const,
        text: s.title,
        seriesId: s.id,
        restricted: s.age_restricted,
      })),
      ...d.authors.slice(0, 3).map((a) => ({ id: `a-${a}`, kind: 'author' as const, text: a })),
      ...d.genres.slice(0, 3).map((g) => ({ id: `g-${g}`, kind: 'genre' as const, text: g })),
      ...d.tags.slice(0, 3).map((t) => ({ id: `t-${t}`, kind: 'tag' as const, text: t })),
    ];
  }, [result.data, enabled]);

  useEffect(() => setActive(-1), [options]);

  // Close on outside pointer.
  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, []);

  const go = (o: Option | null) => {
    setOpen(false);
    if (!o) {
      const q = text.trim();
      if (q) void navigate({ to: '/library/browse', search: { q } });
      return;
    }
    setText('');
    if (o.kind === 'series')
      void navigate({ to: '/library/s/$seriesId', params: { seriesId: o.seriesId! } });
    else if (o.kind === 'genre')
      void navigate({ to: '/library/browse', search: { genres: o.text } });
    else if (o.kind === 'tag') void navigate({ to: '/library/browse', search: { tags: o.text } });
    else void navigate({ to: '/library/browse', search: { q: o.text } });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (options.length === 0) return;
      e.preventDefault();
      setOpen(true);
      const n = options.length;
      setActive((a) => (e.key === 'ArrowDown' ? (a + 1) % n : a <= 0 ? n - 1 : a - 1));
    } else if (e.key === 'Home' && open && options.length) {
      e.preventDefault();
      setActive(0);
    } else if (e.key === 'End' && open && options.length) {
      e.preventDefault();
      setActive(options.length - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      go(open && active >= 0 ? (options[active] ?? null) : null);
    } else if (e.key === 'Escape') {
      if (open) {
        e.preventDefault();
        setOpen(false);
      } else setText('');
    }
  };

  const showList = open && enabled;
  const activeId = active >= 0 && options[active] ? `${uid}-${options[active]!.id}` : undefined;

  return (
    <div className="lib-search" ref={wrap}>
      <Icon name="search" size={16} className="lib-search-icon" />
      <input
        type="search"
        role="combobox"
        className="lib-search-input"
        placeholder="Search manga, authors, and genres…"
        aria-label="Search the library"
        aria-expanded={showList}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={activeId}
        autoComplete="off"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
      />
      <div
        id={listId}
        role="listbox"
        aria-label="Search results"
        className="lib-search-list"
        hidden={!showList}
      >
        {showList && result.isPending && <p className="lib-search-note">Searching…</p>}
        {showList && result.isError && (
          <p className="lib-search-note">Search is unavailable right now.</p>
        )}
        {showList && result.isSuccess && options.length === 0 && (
          <p className="lib-search-note">Nothing found for “{debounced}”.</p>
        )}
        {showList &&
          GROUPS.map((g) => {
            const items = options.filter((o) => o.kind === g.kind);
            if (items.length === 0) return null;
            return (
              <div key={g.kind} role="group" aria-label={g.title}>
                <p className="lib-search-group" aria-hidden="true">
                  {g.title}
                </p>
                {items.map((o) => (
                  <div
                    key={o.id}
                    id={`${uid}-${o.id}`}
                    role="option"
                    aria-selected={options[active]?.id === o.id}
                    className="lib-search-opt"
                    data-active={options[active]?.id === o.id || undefined}
                    onPointerDown={(e) => e.preventDefault()}
                    onClick={() => go(o)}
                  >
                    <span>{o.kind === 'genre' || o.kind === 'tag' ? label(o.text) : o.text}</span>
                    {o.restricted && <AgeBadge />}
                  </div>
                ))}
              </div>
            );
          })}
      </div>
    </div>
  );
}
