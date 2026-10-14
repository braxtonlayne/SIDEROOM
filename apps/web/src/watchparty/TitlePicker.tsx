// Pick a title or an episode to watch together: a search box over the Cinema catalog and, for series, a season
// and episode list. Plain state and effects (no query cache), so it also works inside the Lounge.
import { Button, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useId, useState } from 'react';
import { getEpisodes, searchTitles } from '../cinema/api';
import type { Episode, Title } from '../cinema/types';

export interface PickedTitle {
  titleId: string;
  /** Null for a movie. */
  episodeId: string | null;
  /** "Dark" or "Dark · S1 E2 · Secrets". */
  label: string;
  ageRestricted: boolean;
}

export interface TitlePickerProps {
  onPick: (t: PickedTitle) => void;
  /** Button text, "Add" or "Select". */
  actionLabel?: string;
}

const episodeLabel = (t: Title, e: Episode) =>
  `${t.title} · S${e.season} E${e.number}${e.title ? ` · ${e.title}` : ''}`;

export function TitlePicker({ onPick, actionLabel = 'Select' }: TitlePickerProps) {
  const [q, setQ] = useState('');
  const [results, setResults] = useState<Title[]>([]);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [open, setOpen] = useState<Title | null>(null);
  const listId = useId();

  useEffect(() => {
    const query = q.trim();
    if (query.length < 2) {
      setResults([]);
      setFailed(false);
      return;
    }
    const ctl = new AbortController();
    const t = setTimeout(() => {
      setBusy(true);
      searchTitles(query, 8, ctl.signal)
        .then((r) => {
          setResults(r.titles);
          setFailed(false);
        })
        .catch(() => {
          if (!ctl.signal.aborted) setFailed(true);
        })
        .finally(() => setBusy(false));
    }, 250);
    return () => {
      clearTimeout(t);
      ctl.abort();
    };
  }, [q]);

  if (open) {
    return (
      <EpisodePicker
        title={open}
        onBack={() => setOpen(null)}
        onPick={onPick}
        actionLabel={actionLabel}
      />
    );
  }
  return (
    <div className="wp-picker">
      <TextInput
        label="Search the cinema"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Title, show or film"
        aria-controls={listId}
      />
      <div id={listId} className="wp-picker-list" aria-live="polite">
        {busy && <Spinner label="Searching" />}
        {failed && <p className="wp-muted">Search is unavailable right now.</p>}
        {!busy && !failed && q.trim().length >= 2 && results.length === 0 && (
          <p className="wp-muted">Nothing found.</p>
        )}
        <ul className="wp-list">
          {results.map((t) => (
            <li key={t.id} className="wp-row">
              <span className="wp-row-main">
                <span className="wp-row-title">{t.title}</span>
                <span className="wp-muted">
                  {[t.year, t.kind === 'movie' ? 'Film' : t.kind === 'anime' ? 'Anime' : 'Series']
                    .filter(Boolean)
                    .join(' · ')}
                  {t.age_restricted ? ' · 18+' : ''}
                </span>
              </span>
              {t.kind === 'movie' ? (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`${actionLabel} ${t.title}`}
                  onClick={() =>
                    onPick({
                      titleId: t.id,
                      episodeId: null,
                      label: t.title,
                      ageRestricted: t.age_restricted,
                    })
                  }
                >
                  {actionLabel}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Choose an episode of ${t.title}`}
                  onClick={() => setOpen(t)}
                >
                  Episodes
                </Button>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function EpisodePicker({
  title,
  onBack,
  onPick,
  actionLabel,
}: {
  title: Title;
  onBack: () => void;
  onPick: (t: PickedTitle) => void;
  actionLabel: string;
}) {
  const seasons = Math.max(1, title.season_count ?? 1);
  const [season, setSeason] = useState(1);
  const [eps, setEps] = useState<Episode[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let dead = false;
    setEps(null);
    setFailed(false);
    getEpisodes(title.id, season)
      .then((r) => !dead && setEps(r.items))
      .catch(() => !dead && setFailed(true));
    return () => {
      dead = true;
    };
  }, [title.id, season]);

  return (
    <div className="wp-picker">
      <div className="wp-picker-head">
        <Button size="sm" variant="ghost" onClick={onBack}>
          Back to results
        </Button>
        <strong>{title.title}</strong>
        <label className="wp-inline">
          <span className="wp-muted">Season</span>
          <select
            value={season}
            onChange={(e) => setSeason(Number(e.target.value))}
            aria-label="Season"
          >
            {Array.from({ length: seasons }, (_, i) => i + 1).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
      </div>
      {eps === null && !failed && <Spinner label="Loading episodes" />}
      {failed && <p className="wp-muted">Episodes could not be loaded.</p>}
      <ul className="wp-list">
        {eps?.map((e) => (
          <li key={e.id} className="wp-row">
            <span className="wp-row-main">
              <span className="wp-row-title">
                E{e.number} {e.title}
              </span>
            </span>
            <Button
              size="sm"
              variant="secondary"
              aria-label={`${actionLabel} season ${e.season} episode ${e.number}`}
              onClick={() =>
                onPick({
                  titleId: title.id,
                  episodeId: e.id,
                  label: episodeLabel(title, e),
                  ageRestricted: title.age_restricted,
                })
              }
            >
              {actionLabel}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
