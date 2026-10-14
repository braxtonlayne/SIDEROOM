// Pick a title or an episode to watch together: a search box over the Cinema catalog and, for series, a season
// and episode list. Plain state and effects (no query cache), so it also works inside the Lounge.
import { Button, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useId, useState } from 'react';
import { getEpisodes, searchTitles } from '../cinema/api';
import type { Episode, Title } from '../cinema/types';
import { useT } from '../i18n';

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

export function TitlePicker({ onPick, actionLabel: actionLabelProp }: TitlePickerProps) {
  const tr = useT();
  const actionLabel = actionLabelProp ?? tr('watchparty.titlePicker.select');
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
    const timer = setTimeout(() => {
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
      clearTimeout(timer);
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
        label={tr('watchparty.titlePicker.searchLabel')}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={tr('watchparty.titlePicker.searchPlaceholder')}
        aria-controls={listId}
      />
      <div id={listId} className="wp-picker-list" aria-live="polite">
        {busy && <Spinner label={tr('watchparty.titlePicker.searching')} />}
        {failed && <p className="wp-muted">{tr('watchparty.titlePicker.unavailable')}</p>}
        {!busy && !failed && q.trim().length >= 2 && results.length === 0 && (
          <p className="wp-muted">{tr('watchparty.titlePicker.nothingFound')}</p>
        )}
        <ul className="wp-list">
          {results.map((t) => (
            <li key={t.id} className="wp-row">
              <span className="wp-row-main">
                <span className="wp-row-title">{t.title}</span>
                <span className="wp-muted">
                  {[
                    t.year,
                    t.kind === 'movie'
                      ? tr('watchparty.titlePicker.film')
                      : t.kind === 'anime'
                        ? tr('watchparty.titlePicker.anime')
                        : tr('watchparty.titlePicker.series'),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                  {t.age_restricted ? ' · 18+' : ''}
                </span>
              </span>
              {t.kind === 'movie' ? (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={tr('watchparty.titlePicker.actionTitle', {
                    action: actionLabel,
                    title: t.title,
                  })}
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
                  aria-label={tr('watchparty.titlePicker.chooseEpisode', { title: t.title })}
                  onClick={() => setOpen(t)}
                >
                  {tr('watchparty.titlePicker.episodes')}
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
  const tr = useT();
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
          {tr('watchparty.titlePicker.backToResults')}
        </Button>
        <strong>{title.title}</strong>
        <label className="wp-inline">
          <span className="wp-muted">{tr('watchparty.titlePicker.season')}</span>
          <select
            value={season}
            onChange={(e) => setSeason(Number(e.target.value))}
            aria-label={tr('watchparty.titlePicker.season')}
          >
            {Array.from({ length: seasons }, (_, i) => i + 1).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
      </div>
      {eps === null && !failed && <Spinner label={tr('watchparty.titlePicker.loadingEpisodes')} />}
      {failed && <p className="wp-muted">{tr('watchparty.titlePicker.episodesFailed')}</p>}
      <ul className="wp-list">
        {eps?.map((e) => (
          <li key={e.id} className="wp-row">
            <span className="wp-row-main">
              <span className="wp-row-title">
                {tr('watchparty.titlePicker.episodeLine', { number: e.number, title: e.title })}
              </span>
            </span>
            <Button
              size="sm"
              variant="secondary"
              aria-label={tr('watchparty.titlePicker.actionEpisode', {
                action: actionLabel,
                season: e.season,
                number: e.number,
              })}
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
