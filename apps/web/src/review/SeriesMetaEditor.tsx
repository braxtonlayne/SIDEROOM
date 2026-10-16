// The series metadata editor of a new-series draft (REV-ED-001/005): the fields a reviewer compares, "Fetch
// details" from the providers the operator enabled (the person accepts each field one by one), and the
// warnings that matter before submitting (possible duplicates).
import { Button, Checkbox, Notice, Select, TextArea, TextInput } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import * as p13 from './apiP13';
import * as rv from './api';
import type { LibraryPreview } from './types';

type Fields = {
  title: string;
  alt_titles: string;
  authors: string;
  year: string;
  synopsis: string;
  genres: string;
};

const list = (s: string) =>
  s
    .split(/[,;\n]/)
    .map((x) => x.trim())
    .filter(Boolean);

const fromPreview = (p: LibraryPreview['series']): Fields => ({
  title: p.title,
  alt_titles: p.alt_titles.join(', '),
  authors: p.authors.join(', '),
  year: p.year ? String(p.year) : '',
  synopsis: p.synopsis,
  genres: p.genres.join(', '),
});

type FieldKey = keyof Fields;
const FIELD_LABEL: Record<FieldKey, MessageKey> = {
  title: 'review.lookup.fieldTitle',
  alt_titles: 'review.lookup.fieldTitle',
  authors: 'review.lookup.fieldAuthors',
  year: 'review.lookup.fieldYear',
  synopsis: 'review.lookup.fieldSynopsis',
  genres: 'review.lookup.fieldGenres',
};

/** The fields a lookup result offers, as the strings the form holds. */
export function offered(c: p13.LookupCandidate): Partial<Fields> {
  const out: Partial<Fields> = {};
  if (c.title) out.title = c.title;
  if (c.year) out.year = String(c.year);
  if (c.synopsis) out.synopsis = c.synopsis;
  if (c.authors.length) out.authors = c.authors.join(', ');
  if (c.genres.length) out.genres = c.genres.join(', ');
  return out;
}

/** "Fetch details": search a provider, pick a result, tick the fields to take over. */
export function LookupPanel({
  current,
  section = 'library',
  onApply,
}: {
  current: Partial<Fields>;
  section?: 'library' | 'cinema';
  onApply: (take: Partial<Fields>) => void;
}) {
  const t = useT();
  const providers = useQuery({
    queryKey: ['lookup-providers', section],
    queryFn: () => p13.lookupProviders(section),
    staleTime: 60_000,
  });
  const [provider, setProvider] = useState('');
  const [q, setQ] = useState(current.title ?? '');
  const [results, setResults] = useState<p13.LookupCandidate[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [chosen, setChosen] = useState<p13.LookupCandidate | null>(null);
  const [ticked, setTicked] = useState<Set<FieldKey>>(new Set());
  if (!providers.data?.enabled) return null;
  const active = provider || providers.data.providers[0]?.id || '';
  const search = async () => {
    setBusy(true);
    setFailed(false);
    setChosen(null);
    try {
      const r = await p13.lookup({ provider: active, q: q.trim(), section });
      setResults(r.candidates);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };
  const offer = chosen ? offered(chosen) : {};
  const keys = Object.keys(offer) as FieldKey[];
  const pick = (c: p13.LookupCandidate) => {
    setChosen(c);
    // Empty fields are ticked for the person; fields that already hold text start unticked.
    setTicked(new Set((Object.keys(offered(c)) as FieldKey[]).filter((k) => !current[k]?.trim())));
  };
  return (
    <section className="rv-panel" aria-labelledby="lk-h">
      <h3 id="lk-h">{t('review.lookup.title')}</h3>
      <p className="rv-muted">{t('review.lookup.intro')}</p>
      <form
        className="rv-row"
        onSubmit={(e) => {
          e.preventDefault();
          if (q.trim().length >= 2) void search();
        }}
      >
        {providers.data.providers.length > 1 && (
          <Select
            aria-label={t('review.lookup.provider')}
            options={providers.data.providers.map((p) => ({ value: p.id, label: p.label }))}
            value={active}
            onValueChange={setProvider}
          />
        )}
        <TextInput
          label={t('review.lookup.query')}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <Button type="submit" variant="secondary" loading={busy} disabled={q.trim().length < 2}>
          {t('review.lookup.search')}
        </Button>
      </form>
      {failed && <Notice variant="warning">{t('review.lookup.error')}</Notice>}
      {results && results.length === 0 && <p className="rv-muted">{t('review.lookup.empty')}</p>}
      {results && results.length > 0 && !chosen && (
        <ul className="rv-changes">
          {results.map((c) => (
            <li key={`${c.provider}:${c.external_id}`}>
              <strong>
                {c.title}
                {c.year ? ` (${c.year})` : ''}
              </strong>
              <span>{c.synopsis.slice(0, 160)}</span>
              <Button size="sm" variant="secondary" onClick={() => pick(c)}>
                {t('review.lookup.use', { title: c.title })}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {chosen && (
        <div className="rv-stack">
          <p className="rv-muted">{t('review.lookup.source', { provider: chosen.provider })}</p>
          {keys.map((k) => (
            <div key={k}>
              <Checkbox
                label={t('review.lookup.accept', { field: t(FIELD_LABEL[k]) })}
                checked={ticked.has(k)}
                onChange={(e) => {
                  const next = new Set(ticked);
                  if (e.target.checked) next.add(k);
                  else next.delete(k);
                  setTicked(next);
                }}
              />
              <div>{offer[k]}</div>
              <div className="rv-muted">
                {t('review.lookup.current', {
                  value: current[k]?.trim() || t('review.lookup.blank'),
                })}
              </div>
            </div>
          ))}
          <div className="rv-row">
            <Button
              disabled={ticked.size === 0}
              onClick={() => {
                const take: Partial<Fields> = {};
                for (const k of ticked) take[k] = offer[k]!;
                onApply(take);
                setChosen(null);
                setResults(null);
              }}
            >
              {t('review.lookup.apply', { count: ticked.size })}
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

/** Edit the draft series; "Fetch details" fills it in field by field. */
export function SeriesMetaEditor({
  submissionId,
  preview,
  onSaved,
}: {
  submissionId: string;
  preview: LibraryPreview;
  onSaved: () => void;
}) {
  const t = useT();
  const s = preview.series;
  const [f, setF] = useState<Fields>(() => fromPreview(s));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const set = (patch: Partial<Fields>) => {
    setDone(false);
    setF((x) => ({ ...x, ...patch }));
  };
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const year = f.year.trim() ? Number(f.year) : null;
      await rv.updateSeries(submissionId, {
        kind: s.kind,
        title: f.title.trim(),
        alt_titles: list(f.alt_titles),
        authors: list(f.authors),
        artists: s.artists,
        status: s.status,
        year: year !== null && Number.isFinite(year) ? year : null,
        synopsis: f.synopsis.trim(),
        genres: list(f.genres),
        tags: s.tags,
        reading_direction: s.reading_direction,
        age_restricted: s.age_restricted,
      });
      setDone(true);
      onSaved();
    } catch {
      setError(t('review.meta.saveError'));
    } finally {
      setSaving(false);
    }
  };
  return (
    <>
      <LookupPanel current={f} onApply={set} />
      <form
        className="rv-panel rv-form"
        aria-label={t('review.meta.title')}
        onSubmit={(e) => {
          e.preventDefault();
          if (f.title.trim()) void save();
        }}
      >
        <h3>{t('review.meta.title')}</h3>
        <TextInput
          label={t('review.meta.seriesTitle')}
          value={f.title}
          maxLength={300}
          onChange={(e) => set({ title: e.target.value })}
        />
        <TextInput
          label={t('review.meta.altTitles')}
          hint={t('review.meta.commas')}
          value={f.alt_titles}
          onChange={(e) => set({ alt_titles: e.target.value })}
        />
        <TextInput
          label={t('review.meta.authors')}
          hint={t('review.meta.commas')}
          value={f.authors}
          onChange={(e) => set({ authors: e.target.value })}
        />
        <TextInput
          label={t('review.meta.year')}
          inputMode="numeric"
          value={f.year}
          onChange={(e) => set({ year: e.target.value })}
        />
        <TextInput
          label={t('review.meta.genres')}
          hint={t('review.meta.commas')}
          value={f.genres}
          onChange={(e) => set({ genres: e.target.value })}
        />
        <TextArea
          label={t('review.meta.synopsis')}
          rows={5}
          maxLength={10000}
          value={f.synopsis}
          onChange={(e) => set({ synopsis: e.target.value })}
        />
        {error && <p role="alert">{error}</p>}
        {done && <p role="status">{t('review.meta.saved')}</p>}
        <div>
          <Button type="submit" variant="secondary" loading={saving} disabled={!f.title.trim()}>
            {t('review.meta.save')}
          </Button>
        </div>
      </form>
    </>
  );
}

/** Possible duplicates of my draft, as a warning before submitting. */
export function DuplicateWarning({ submissionId }: { submissionId: string }) {
  const t = useT();
  const d = useQuery({
    queryKey: ['dup-mine', submissionId],
    queryFn: () => p13.duplicatesMine(submissionId),
  });
  if (!d.data || d.data.matches.length === 0) return null;
  return (
    <Notice variant="warning" title={t('review.meta.dupTitle')}>
      <ul>
        {d.data.matches.map((m) => (
          <li key={m.ref_id}>
            {m.href ? <a href={m.href}>{m.title}</a> : m.title}
            {m.year ? ` (${m.year})` : ''}
          </li>
        ))}
      </ul>
      {t('review.meta.dupHint')}
    </Notice>
  );
}
