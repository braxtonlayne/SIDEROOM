// Series page (LIB-SER-001, 002 partly, 003, 004): header, actions, edition selector and chapter list.
import { Button, EmptyState, Icon, Notice, Skeleton, Tabs } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { AgeRestrictionButton } from '../moderation/AgeRestrictionButton';
import { ReportButton } from '../moderation/reports';
import { followSeries, getChapters, getSeries, keys, markReadUpTo, unfollowSeries } from './api';
import { pickContinue, progressOf, readMark, type ProgressMap } from './continue';
import { progressApi } from './progress';
import { CommentsSection } from './Comments';
import { AddToListButton, RatingBlock, ReviewsTab } from './SeriesSocial';
import { ChapterDownload, ContributorsTab, RelatedTab, ShareButton } from './SeriesExtras';
import type { Chapter, Edition, Series, SeriesDetail, Volume } from './types';
import { AgeBadge, formatDate, KIND_LABEL, label, languageName, STATUS_LABEL } from './ui';

export function SeriesPage({ seriesId }: { seriesId: string }) {
  const t = useT();
  const detail = useQuery({ queryKey: keys.series(seriesId), queryFn: () => getSeries(seriesId) });
  if (detail.isPending)
    return (
      <div className="lib-series" aria-busy="true">
        <Skeleton height={320} />
      </div>
    );
  if (detail.isError) {
    const gone = detail.error instanceof ApiError && detail.error.status === 404;
    return (
      <EmptyState
        title={gone ? t('library.series.goneTitle') : t('library.series.errorTitle')}
        description={
          gone ? t('library.series.goneDescription') : t('library.series.errorDescription')
        }
        action={
          <Link to="/library/browse" search={{}} className="lib-btn">
            {t('library.series.browse')}
          </Link>
        }
      />
    );
  }
  return <SeriesView detail={detail.data} />;
}

function SeriesView({ detail }: { detail: SeriesDetail }) {
  const t = useT();
  const { series, editions } = detail;
  const defaultEdition = editions.find((e) => e.is_default) ?? editions[0];
  const [editionId, setEditionId] = useState<string | undefined>(defaultEdition?.id);
  const [tab, setTab] = useState('chapters');

  return (
    <article className="lib-series">
      <Header
        series={series}
        follow={detail.follow ?? null}
        editionId={editionId}
        editions={editions}
        onEdition={setEditionId}
      />
      <Tabs
        label={t('library.series.sections')}
        value={tab}
        onValueChange={setTab}
        items={[
          {
            id: 'chapters',
            label: t('library.series.tab.chapters'),
            content: <ChapterList series={series} editionId={editionId} />,
          },
          {
            id: 'reviews',
            label: t('library.series.tab.reviews'),
            content: <ReviewsTab seriesId={series.id} />,
          },
          {
            id: 'comments',
            label: t('library.series.tab.comments'),
            content: <CommentsSection target={{ kind: 'series', id: series.id }} />,
          },
          {
            id: 'related',
            label: t('library.series.tab.related'),
            content: <RelatedTab seriesId={series.id} />,
          },
          {
            id: 'contributors',
            label: t('library.series.tab.contributors'),
            content: <ContributorsTab seriesId={series.id} />,
          },
        ]}
      />
    </article>
  );
}

/* ------------------------------------------------------------------ header */

function Header({
  series,
  follow,
  editionId,
  editions,
  onEdition,
}: {
  series: Series;
  follow: SeriesDetail['follow'];
  editionId: string | undefined;
  editions: Edition[];
  onEdition: (id: string) => void;
}) {
  const t = useT();
  const [more, setMore] = useState(false);
  const people = [...new Set([...series.authors, ...series.artists])];
  const long = series.synopsis.length > 280;
  return (
    <header className="lib-series-head">
      {series.cover_url && (
        <div className="lib-series-backdrop" aria-hidden="true">
          <img src={series.cover_url} alt="" />
        </div>
      )}
      <div className="lib-series-cover">
        {series.cover_url ? (
          <img src={series.cover_url} alt={t('library.series.coverAlt', { title: series.title })} />
        ) : (
          <span aria-hidden="true">{series.title.charAt(0)}</span>
        )}
      </div>
      <div className="lib-series-info">
        <p className="lib-eyebrow">
          {KIND_LABEL[series.kind]} · {STATUS_LABEL[series.status]}
          {series.year ? ` · ${series.year}` : ''}
        </p>
        <h1 className="lib-h1">
          {series.title} {series.age_restricted && <AgeBadge />}
        </h1>
        {series.alt_titles.length > 0 && (
          <p className="lib-muted">
            {t('library.series.alsoKnownAs', { titles: series.alt_titles.join(' · ') })}
          </p>
        )}
        {people.length > 0 && <p className="lib-people">{people.join(', ')}</p>}
        <ul className="lib-chip-list" aria-label={t('library.series.genresAndTags')}>
          {series.genres.map((g) => (
            <li key={`g-${g}`}>
              <Link to="/library/browse" search={{ genres: g }}>
                {label(g)}
              </Link>
            </li>
          ))}
          {series.tags.map((tag) => (
            <li key={`t-${tag}`} className="lib-chip-tag">
              <Link to="/library/browse" search={{ tags: tag }}>
                {label(tag)}
              </Link>
            </li>
          ))}
        </ul>
        <RatingBlock seriesId={series.id} />
        {series.synopsis && (
          <div>
            <p className={`lib-synopsis${more || !long ? '' : ' lib-clamp'}`}>{series.synopsis}</p>
            {long && (
              <button
                type="button"
                className="lib-linklike"
                aria-expanded={more}
                onClick={() => setMore((v) => !v)}
              >
                {more ? t('library.series.showLess') : t('library.series.readMore')}
              </button>
            )}
          </div>
        )}
        <Actions series={series} follow={follow} editionId={editionId} />
        {editions.length > 1 && (
          <label className="lib-select lib-edition">
            <span>{t('library.series.edition')}</span>
            <select value={editionId} onChange={(e) => onEdition(e.target.value)}>
              {editions.map((e) => (
                <option key={e.id} value={e.id}>
                  {t('library.series.editionOption', {
                    label: e.label,
                    lang: languageName(e.language),
                  })}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </header>
  );
}

function useSeriesProgress(series: Series, editionId: string | undefined) {
  const chapters = useQuery({
    queryKey: keys.chapters(series.id, editionId),
    queryFn: () => getChapters(series.id, editionId),
    enabled: !!editionId,
  });
  const local = useQuery({
    queryKey: ['library', 'series-progress', series.id],
    queryFn: () => progressApi.forSeries(series.id),
  });
  const map: ProgressMap = local.data ?? {};
  return { chapters, map };
}

function Actions({
  series,
  follow,
  editionId,
}: {
  series: Series;
  follow: SeriesDetail['follow'];
  editionId: string | undefined;
}) {
  const t = useT();
  const qc = useQueryClient();
  const { chapters, map } = useSeriesProgress(series, editionId);
  const target = useMemo(
    () => pickContinue(chapters.data?.chapters ?? [], map),
    [chapters.data, map],
  );
  const following = !!follow;
  const toggle = useMutation({
    mutationFn: async () => {
      if (following) await unfollowSeries(series.id);
      else await followSeries(series.id);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.series(series.id) });
      void qc.invalidateQueries({ queryKey: ['library', 'follows'] });
    },
  });
  return (
    <div className="lib-actions">
      {target ? (
        <Link
          to="/library/r/$chapterId/{-$page}"
          params={{
            chapterId: target.chapter.id,
            page: target.pageIndex !== null ? String(target.pageIndex + 1) : undefined,
          }}
          className="lib-btn lib-btn-gold"
        >
          <Icon name="book" size={16} />
          {target.mode === 'start'
            ? t('library.series.startReading')
            : t('library.series.continueChapter', { n: target.chapter.number })}
        </Link>
      ) : (
        <span className="lib-btn" aria-disabled="true">
          {chapters.isPending
            ? t('library.series.loadingChapters')
            : t('library.series.noChapters')}
        </span>
      )}
      <Button
        variant="secondary"
        aria-pressed={following}
        disabled={toggle.isPending}
        onClick={() => toggle.mutate()}
      >
        <Icon name="bell" size={16} />
        {following ? t('library.series.following') : t('library.series.follow')}
      </Button>
      <AddToListButton seriesId={series.id} />
      <ShareButton seriesId={series.id} title={series.title} />
      {target && (
        <ChapterDownload
          chapterId={target.chapter.id}
          label={t('library.series.downloadChapter', { n: target.chapter.number })}
        />
      )}
      <ReportButton target={{ kind: 'library', id: series.id }} />
      <AgeRestrictionButton
        kind="library_series"
        id={series.id}
        restricted={series.age_restricted}
        onChanged={() => void qc.invalidateQueries({ queryKey: ['library'] })}
      />
      {toggle.isError && <Notice variant="danger">{t('library.series.followError')}</Notice>}
    </div>
  );
}

/* ---------------------------------------------------------------- chapters */

function ChapterList({ series, editionId }: { series: Series; editionId: string | undefined }) {
  const t = useT();
  const { chapters, map } = useSeriesProgress(series, editionId);
  const [desc, setDesc] = useState(true);
  const data = chapters.data;
  const qc = useQueryClient();
  const markUpTo = useMutation({
    mutationFn: (chapterId: string) => markReadUpTo(chapterId, { read: true, up_to: true }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['library', 'chapters', series.id] });
      void qc.invalidateQueries({ queryKey: ['library', 'unread'] });
      void qc.invalidateQueries({ queryKey: ['library', 'recent'] });
    },
  });

  const groups = useMemo(() => {
    const vols = new Map<string, Volume>((data?.volumes ?? []).map((v) => [v.id, v]));
    const by = new Map<string | null, Chapter[]>();
    for (const c of data?.chapters ?? []) {
      const k = c.volume_id && vols.has(c.volume_id) ? c.volume_id : null;
      by.set(k, [...(by.get(k) ?? []), c]);
    }
    const order = (a: number, b: number) => (desc ? b - a : a - b);
    const out = [...by.entries()].map(([id, list]) => ({
      id,
      volume: id ? vols.get(id)! : null,
      chapters: list.sort((a, b) => order(a.number, b.number)),
    }));
    // Volumes by number; chapters outside any volume go last.
    return out.sort((a, b) =>
      a.volume && b.volume ? order(a.volume.number, b.volume.number) : a.volume ? -1 : 1,
    );
  }, [data, desc]);

  if (!editionId || chapters.isPending) return <Skeleton height={160} />;
  if (chapters.isError)
    return <Notice variant="danger" title={t('library.series.chaptersError')} />;
  if ((data?.chapters.length ?? 0) === 0)
    return (
      <EmptyState
        title={t('library.series.noChapters')}
        description={t('library.series.checkBack')}
      />
    );

  return (
    <section aria-label={t('library.series.tab.chapters')}>
      <div className="lib-chapters-head">
        <h2 className="lib-h2">{t('library.series.chapterCount', { n: data!.chapters.length })}</h2>
        <button type="button" className="lib-linklike" onClick={() => setDesc((v) => !v)}>
          {desc ? t('library.series.newestFirst') : t('library.series.oldestFirst')}
        </button>
      </div>
      {groups.map((g) => (
        <div key={g.id ?? 'none'} className="lib-volume">
          {g.volume && (
            <h3 className="lib-volume-title">
              {t('library.series.volume', { n: g.volume.number })}
              {g.volume.title ? t('library.series.titleSuffix', { title: g.volume.title }) : ''}
            </h3>
          )}
          <ul className="lib-chapter-list">
            {g.chapters.map((c) => {
              const p = progressOf(c, map);
              return (
                <li key={c.id} className="lib-chapter-row" data-read={p?.completed || undefined}>
                  <Link
                    to="/library/r/$chapterId/{-$page}"
                    params={{ chapterId: c.id }}
                    className="lib-chapter-link"
                  >
                    <span className="lib-chapter-title">
                      {t('library.series.chapter', { n: c.number })}
                      {c.title ? t('library.series.titleSuffix', { title: c.title }) : ''}
                    </span>
                    <span className="lib-chapter-meta">
                      {[c.release_group, formatDate(c.published_at_ms)].filter(Boolean).join(' · ')}
                    </span>
                    <span className="lib-chapter-mark">
                      {p?.completed && <Icon name="check" size={16} />}
                      {readMark(c, map)}
                    </span>
                  </Link>
                  <ChapterDownload
                    chapterId={c.id}
                    compact
                    label={t('library.series.downloadChapterN', { n: c.number })}
                  />
                  <button
                    type="button"
                    className="lib-chapter-upto"
                    disabled={markUpTo.isPending}
                    title={t('library.series.markReadUpTo')}
                    aria-label={t('library.series.markReadUpToChapter', { n: c.number })}
                    onClick={() => markUpTo.mutate(c.id)}
                  >
                    <Icon name="check" size={16} />
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </section>
  );
}
