// CIN-UI-003: search overlay with instant results (posters), combobox keyboard model, and a filters shortcut.
// Focus stays in the input; arrows move the active result, Enter opens it (or the full results page), Esc closes.
import { Modal } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useId, useState } from 'react';
import { keys, searchTitles } from './api';
import { AgeBadge, KIND_LABEL, titleHref, useOpen } from './ui';

export const SEARCH_DEBOUNCE_MS = 150;

export function SearchOverlay({ open, onClose }: { open: boolean; onClose: () => void }) {
  const uid = useId();
  const go = useOpen();
  const [text, setText] = useState('');
  const [debounced, setDebounced] = useState('');
  const [active, setActive] = useState(-1);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(text.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [text]);
  useEffect(() => {
    if (!open) {
      setText('');
      setDebounced('');
    }
  }, [open]);

  const enabled = open && debounced.length >= 2;
  const result = useQuery({
    queryKey: keys.search(debounced),
    queryFn: ({ signal }) => searchTitles(debounced, 8, signal),
    enabled,
    staleTime: 30_000,
  });
  const titles = enabled ? (result.data?.titles ?? []) : [];
  useEffect(() => setActive(-1), [titles.length, debounced]);

  const openAll = () => {
    const q = text.trim();
    onClose();
    if (q) void go(`/cinema/search?q=${encodeURIComponent(q)}`);
  };
  const openTitle = (i: number) => {
    const t = titles[i];
    if (!t) return;
    onClose();
    void go(titleHref(t.id), t.age_restricted);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const n = titles.length;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (!n) return;
      e.preventDefault();
      setActive((a) => (e.key === 'ArrowDown' ? (a + 1) % n : a <= 0 ? n - 1 : a - 1));
    } else if (e.key === 'Home' && n) {
      e.preventDefault();
      setActive(0);
    } else if (e.key === 'End' && n) {
      e.preventDefault();
      setActive(n - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (active >= 0) openTitle(active);
      else openAll();
    }
  };

  const listId = `${uid}-list`;
  return (
    <Modal open={open} onClose={onClose} title="Search the cinema" size="lg">
      <div className="cin-search">
        <input
          type="search"
          role="combobox"
          className="cin-search-input"
          placeholder="Search films, series and anime…"
          aria-label="Search the cinema"
          aria-expanded={enabled && titles.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={active >= 0 ? `${uid}-o${active}` : undefined}
          autoComplete="off"
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
        />
        <div id={listId} role="listbox" aria-label="Search results" className="cin-search-list">
          {enabled && result.isPending && <p className="cin-search-note">Searching…</p>}
          {enabled && result.isError && (
            <p className="cin-search-note">Search is unavailable right now.</p>
          )}
          {enabled && result.isSuccess && titles.length === 0 && (
            <p className="cin-search-note">Nothing found for “{debounced}”.</p>
          )}
          {titles.map((t, i) => (
            <div
              key={t.id}
              id={`${uid}-o${i}`}
              role="option"
              aria-selected={i === active}
              data-active={i === active || undefined}
              className="cin-search-opt"
              onPointerDown={(e) => e.preventDefault()}
              onClick={() => openTitle(i)}
            >
              {t.poster_url ? (
                <img src={t.poster_url} alt="" className="cin-search-poster" />
              ) : (
                <span className="cin-search-poster" aria-hidden="true" />
              )}
              <span className="cin-search-text">
                <span className="cin-search-title">{t.title}</span>
                <span className="cin-muted">
                  {[t.year, KIND_LABEL[t.kind]].filter(Boolean).join(' · ')}
                </span>
              </span>
              {t.age_restricted && <AgeBadge />}
            </div>
          ))}
        </div>
        <p className="cin-search-foot">
          <button type="button" className="cin-link-btn" onClick={openAll} disabled={!text.trim()}>
            See all results
          </button>
          <button
            type="button"
            className="cin-link-btn"
            onClick={() => {
              onClose();
              void go('/cinema/movies?filters=1');
            }}
          >
            Filters
          </button>
        </p>
      </div>
    </Modal>
  );
}
