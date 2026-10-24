// Cinema > Submit (RDY-MED-002): my video submissions with their review states, and a form to start one
// (a film, a series or an anime, or new episodes for a series that is already in the catalog). The
// submission page is `DraftPage`. Same pattern as the Library's submissions list.
import { Button, EmptyState, Notice, Select, Spinner, TextArea, TextInput } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import * as rv from '../../review/api';
import { errorMessages } from '../../review/api';
import { NamingGuide } from '../../review/NamingGuide';
import { StateChip, fmtWhen, useSubmissionUpdates } from '../../review/shared';
import { searchTitles } from '../api';
import type { Title } from '../types';

const KINDS = 'cinema.title,cinema.episodes';

const FILTERS: { value: string; labelKey: MessageKey }[] = [
  { value: '', labelKey: 'cinema.submit.filter.all' },
  { value: 'draft', labelKey: 'cinema.submit.filter.draft' },
  { value: 'submitted,in_review', labelKey: 'cinema.submit.filter.waiting' },
  { value: 'changes_requested', labelKey: 'cinema.submit.filter.changes' },
  { value: 'approved,published', labelKey: 'cinema.submit.filter.published' },
  { value: 'rejected,withdrawn', labelKey: 'cinema.submit.filter.closed' },
];

function Problems({ messages }: { messages: string[] }) {
  if (messages.length === 0) return null;
  return (
    <Notice variant="danger" role="alert">
      {messages.join(' ')}
    </Notice>
  );
}

export function CinemaSubmitPage() {
  const t = useT();
  const qc = useQueryClient();
  useSubmissionUpdates(() => void qc.invalidateQueries({ queryKey: ['cinema-submit'] }));
  const [filter, setFilter] = useState('');
  const [creating, setCreating] = useState(false);
  const list = useQuery({
    queryKey: ['cinema-submit', 'list', filter],
    queryFn: () =>
      rv.listMine({
        kind: KINDS,
        sort: 'updated',
        order: 'desc',
        limit: 50,
        ...(filter ? { state: filter } : {}),
      }),
  });
  return (
    <div className="cin-submit">
      <div className="rv-head">
        <h1 className="cin-h1">{t('cinema.submit.title')}</h1>
        <div className="rv-row">
          <Select
            aria-label={t('cinema.submit.filterLabel')}
            options={FILTERS.map((f) => ({ value: f.value, label: t(f.labelKey) }))}
            value={filter}
            onValueChange={setFilter}
          />
          <Button onClick={() => setCreating((c) => !c)}>
            {creating ? t('cinema.submit.closeForm') : t('cinema.submit.new')}
          </Button>
        </div>
      </div>
      <p className="rv-muted">{t('cinema.submit.intro')}</p>
      <NamingGuide section="cinema" />
      {creating && <NewSubmission />}
      {list.isPending && <Spinner label={t('cinema.submit.loadingList')} />}
      {list.isError && (
        <Notice variant="danger" role="alert">
          {t('cinema.submit.listError')}
        </Notice>
      )}
      {list.data && list.data.items.length === 0 && (
        <EmptyState
          title={t('cinema.submit.emptyTitle')}
          description={t('cinema.submit.emptyDescription')}
        />
      )}
      {list.data && list.data.items.length > 0 && (
        <ul
          className="rv-stack"
          style={{ listStyle: 'none', padding: 0 }}
          aria-label={t('cinema.submit.listLabel')}
        >
          {list.data.items.map((s) => (
            <li key={s.id} className="rv-panel" style={{ marginBottom: 0 }}>
              <div className="rv-head" style={{ marginBottom: 4 }}>
                <h2 style={{ fontSize: '1.125rem' }}>
                  <Link to="/cinema/submit/$submissionId" params={{ submissionId: s.id }}>
                    {s.title}
                  </Link>
                </h2>
                <StateChip state={s.state} />
              </div>
              <p className="rv-muted" style={{ margin: 0 }}>
                {t('cinema.submit.kindUpdated', {
                  kind:
                    s.kind === 'cinema.episodes'
                      ? t('cinema.submit.kindEpisodes')
                      : t('cinema.submit.kindTitle'),
                  when: fmtWhen(s.updated_at_ms),
                })}
              </p>
              {s.state === 'changes_requested' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>{t('cinema.submit.reviewerNote')}</strong> {s.decision_reason}
                </p>
              )}
              {s.state === 'rejected' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>{t('cinema.submit.why')}</strong> {s.decision_reason}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

type Mode = 'movie' | 'series' | 'anime' | 'episodes';

function NewSubmission() {
  const t = useT();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [mode, setMode] = useState<Mode>('series');
  const [title, setTitle] = useState('');
  const [year, setYear] = useState('');
  const [synopsis, setSynopsis] = useState('');
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<Title | null>(null);
  const found = useQuery({
    queryKey: ['cinema-submit', 'find', query],
    queryFn: () => searchTitles(query.trim(), 8),
    enabled: mode === 'episodes' && query.trim().length >= 2,
  });
  const create = useMutation({
    mutationFn: () =>
      rv.createSubmission(
        mode === 'episodes'
          ? {
              kind: 'cinema.episodes',
              payload: { title_id: picked!.id, season: 1, episodes: [] },
            }
          : {
              kind: 'cinema.title',
              payload: {
                kind: mode,
                title: title.trim(),
                ...(/^\d{4}$/.test(year.trim()) ? { year: Number(year.trim()) } : {}),
                synopsis: synopsis.trim(),
              },
            },
      ),
    onSuccess: (d) => {
      void qc.invalidateQueries({ queryKey: ['cinema-submit'] });
      void navigate({
        to: '/cinema/submit/$submissionId',
        params: { submissionId: d.submission.id },
      });
    },
  });
  const ready = mode === 'episodes' ? picked !== null : title.trim().length > 0;
  const series = (found.data?.titles ?? []).filter((x) => x.kind !== 'movie');
  return (
    <form
      className="rv-panel rv-form"
      aria-label={t('cinema.submit.new')}
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) create.mutate();
      }}
    >
      <Select
        aria-label={t('cinema.submit.form.what')}
        options={[
          { value: 'movie', label: t('cinema.submit.form.movie') },
          { value: 'series', label: t('cinema.submit.form.series') },
          { value: 'anime', label: t('cinema.submit.form.anime') },
          { value: 'episodes', label: t('cinema.submit.form.episodes') },
        ]}
        value={mode}
        onValueChange={(v) => setMode(v as Mode)}
      />
      {mode === 'episodes' ? (
        <>
          <TextInput
            label={t('cinema.submit.form.findSeries')}
            hint={t('cinema.submit.form.findSeriesHint')}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setPicked(null);
            }}
          />
          {picked && <p role="status">{t('cinema.submit.form.picked', { title: picked.title })}</p>}
          {!picked && series.length > 0 && (
            <ul className="rv-changes" aria-label={t('cinema.submit.form.results')}>
              {series.map((x) => (
                <li key={x.id}>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setPicked(x)}
                    aria-label={t('cinema.submit.form.pick', { title: x.title })}
                  >
                    {x.title}
                    {x.year ? ` (${x.year})` : ''}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : (
        <>
          <TextInput
            label={t('cinema.submit.form.title')}
            value={title}
            maxLength={200}
            required
            onChange={(e) => setTitle(e.target.value)}
          />
          <TextInput
            label={t('cinema.submit.form.year')}
            inputMode="numeric"
            value={year}
            onChange={(e) => setYear(e.target.value)}
          />
          <TextArea
            label={t('cinema.submit.form.synopsis')}
            value={synopsis}
            rows={4}
            onChange={(e) => setSynopsis(e.target.value)}
          />
        </>
      )}
      {create.isError && <Problems messages={errorMessages(create.error)} />}
      <div>
        <Button type="submit" disabled={!ready} loading={create.isPending}>
          {t('cinema.submit.form.start')}
        </Button>
      </div>
    </form>
  );
}
