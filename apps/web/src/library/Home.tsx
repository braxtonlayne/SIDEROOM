// Library home (LIB-HOME-001..009), laid out as docs/assets/mockups/library-home.jpg.
import {
  ArrowLink,
  DisplayHeading,
  EmptyState,
  Eyebrow,
  Icon,
  ProgressBar,
  SegmentedControl,
} from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { useT } from '../i18n';
import { browseSeries, getChapters, getUpdates, keys } from './api';
import { getRecommendations } from './p13api';
import { progressApi, progressLine } from './progress';
import { getFeaturedLists, getRankings, socialKeys, type RankingScope } from './social';
import type { Series } from './types';
import {
  AgeBadge,
  CardSkeletons,
  KIND_LABEL,
  label,
  SeriesCard,
  STATUS_LABEL,
  updateLine,
} from './ui';

const DEFAULT_GENRES = [
  'action',
  'fantasy',
  'romance',
  'slice of life',
  'mystery',
  'comedy',
  'adventure',
];

export function Home() {
  const t = useT();
  return (
    <div className="lib-home">
      <div className="lib-home-head">
        <DisplayHeading size="lg">{t('library.home.title')}</DisplayHeading>
        <p className="lib-sub">{t('library.home.subtitle')}</p>
      </div>
      <Spotlight />
      <PickUp />
      <Trending />
      <Because />
      <Curiosity />
      <div className="lib-two-col">
        <Fresh />
        <Inspiration />
      </div>
      <Finished />
      <HomeFooter />
    </div>
  );
}

/* ---------------------------------------------------------- spotlight */

export function Spotlight() {
  const t = useT();
  // Staff curation (Collections & Spotlight) is a later item; until then the most recently updated series.
  const q = useQuery({
    queryKey: keys.browse({ sort: 'updated', limit: 5 }),
    queryFn: () => browseSeries({ sort: 'updated', limit: 5 }),
  });
  const slides = useMemo(() => (q.data?.items ?? []).slice(0, 5), [q.data]);
  const [at, setAt] = useState(0);
  const s = slides[Math.min(at, Math.max(0, slides.length - 1))];
  const chapters = useQuery({
    queryKey: keys.chapters(s?.id ?? '', undefined),
    queryFn: () => getChapters(s!.id),
    enabled: !!s,
  });
  const first = useMemo(
    () => [...(chapters.data?.chapters ?? [])].sort((a, b) => a.number - b.number)[0],
    [chapters.data],
  );
  if (q.isPending)
    return <div className="lib-spotlight lib-spotlight-skeleton" aria-hidden="true" />;
  if (!s) return null;
  return (
    <section
      className="lib-spotlight"
      aria-roledescription="carousel"
      aria-label={t('library.home.spotlight.label')}
    >
      {s.cover_url && (
        <div className="lib-spotlight-art" aria-hidden="true">
          <img src={s.cover_url} alt="" />
        </div>
      )}
      <div className="lib-spotlight-body" aria-live="polite">
        <Eyebrow>{t('library.home.spotlight.label')}</Eyebrow>
        <DisplayHeading as="h2" size="md">
          {s.title}
        </DisplayHeading>
        <p className="lib-spotlight-meta">
          <span>
            {KIND_LABEL[s.kind]} · {STATUS_LABEL[s.status]}
          </span>
          {s.age_restricted && <AgeBadge />}
        </p>
        <ul className="lib-chip-list" aria-label={t('library.home.spotlight.genres')}>
          {s.genres.slice(0, 3).map((g) => (
            <li key={g}>{label(g)}</li>
          ))}
        </ul>
        <p className="lib-spotlight-tag">{tagline(s)}</p>
        <div className="lib-actions">
          {first ? (
            <Link
              to="/library/r/$chapterId/{-$page}"
              params={{ chapterId: first.id }}
              className="lib-btn lib-btn-gold"
            >
              <Icon name="book" size={16} />
              {t('library.home.spotlight.startReading')}
            </Link>
          ) : null}
          <Link to="/library/s/$seriesId" params={{ seriesId: s.id }} className="lib-btn">
            {t('library.home.spotlight.viewDetails')}
          </Link>
        </div>
      </div>
      <p className="lib-spotlight-caption">{t('library.home.spotlight.caption')}</p>
      {slides.length > 1 && (
        <div className="lib-dots" role="group" aria-label={t('library.home.spotlight.choose')}>
          {slides.map((x, i) => (
            <button
              key={x.id}
              type="button"
              className="lib-dot"
              aria-label={t('library.home.spotlight.show', { title: x.title })}
              aria-current={i === at || undefined}
              onClick={() => setAt(i)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

const tagline = (s: Series) => {
  const first = s.synopsis.trim().split(/(?<=[.!?])\s/)[0] ?? '';
  return first.length > 140 ? `${first.slice(0, 137)}…` : first;
};

/* ------------------------------------------------------------ pick up */

function PickUp() {
  const t = useT();
  const q = useQuery({
    queryKey: ['library', 'recent'],
    queryFn: () => progressApi.recent(3),
  });
  const items = q.data ?? [];
  if (items.length === 0) return null;
  return (
    <section className="lib-section" aria-labelledby="lib-pickup">
      <div className="lib-section-head">
        <h2 id="lib-pickup" className="lib-h2">
          {t('library.home.pickUp.title')}
        </h2>
        <ArrowLink href="/library/history" arrow="→">
          {t('library.home.pickUp.history')}
        </ArrowLink>
      </div>
      <ul className="lib-pickup-list">
        {items.map((i) => (
          <li key={i.chapterId} className="lib-pickup-card">
            <Link
              to="/library/r/$chapterId/{-$page}"
              params={{ chapterId: i.chapterId, page: String(i.pageIndex + 1) }}
              className="lib-pickup-link"
              aria-label={t('library.home.pickUp.continue', { title: i.seriesTitle })}
            >
              <span className="lib-pickup-cover">
                {i.coverUrl ? (
                  <img src={i.coverUrl} alt="" />
                ) : (
                  <span>{i.seriesTitle.charAt(0)}</span>
                )}
              </span>
              <span className="lib-pickup-text">
                <span className="lib-pickup-title">{i.seriesTitle}</span>
                <span className="lib-pickup-line">{progressLine(i)}</span>
                <ProgressBar
                  value={i.percent}
                  label={t('library.home.pickUp.progress', { title: i.seriesTitle })}
                  size={3}
                />
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ----------------------------------------------------------- trending */

function Trending() {
  const t = useT();
  const [scope, setScope] = useState<RankingScope>('global');
  const q = useQuery({
    queryKey: scope === 'global' ? keys.browse({ sort: 'followers', limit: 5 }) : socialKeys.rankings('week', undefined, scope),
    queryFn: async () =>
      scope === 'global'
        ? browseSeries({ sort: 'followers', limit: 5 })
        : { items: (await getRankings({ period: 'week', scope, limit: 5 })).items.map((r) => r.series) },
  });
  return (
    <section className="lib-section" aria-labelledby="lib-trending">
      <div className="lib-section-head">
        <div className="lib-section-title">
          <h2 id="lib-trending" className="lib-h2">
            {t('library.home.trending.title')}
          </h2>
          <span className="lib-section-note">{t('library.home.trending.note')}</span>
        </div>
        <SegmentedControl
          label={t('library.home.trending.scope')}
          size="sm"
          value={scope}
          onValueChange={(v) => setScope(v as RankingScope)}
          options={[
            { value: 'friends', label: t('library.home.trending.friends') },
            { value: 'known', label: t('library.home.trending.known') },
            { value: 'global', label: t('library.home.trending.global') },
          ]}
        />
        <ArrowLink href="/library/rankings" arrow="→">
          {t('library.home.trending.rankings')}
        </ArrowLink>
      </div>
      <SeriesRow query={q} empty={t('library.home.trending.empty')} tagFirst />
    </section>
  );
}

function SeriesRow({
  query,
  empty,
  tagFirst,
}: {
  query: { isPending: boolean; isError: boolean; data?: { items: Series[] } };
  empty: string;
  tagFirst?: boolean;
}) {
  const t = useT();
  if (query.isPending) return <CardSkeletons />;
  if (query.isError) return <p className="lib-muted">{t('library.home.row.error')}</p>;
  const items = query.data?.items ?? [];
  if (items.length === 0) return <p className="lib-muted">{empty}</p>;
  return (
    <div className="lib-poster-row">
      {items.map((s, i) => (
        <SeriesCard
          key={s.id}
          series={s}
          tag={tagFirst && i === 0 ? t('library.home.row.trendingTag') : undefined}
        />
      ))}
    </div>
  );
}

/* ------------------------------------------------- because you read */

/** "Because you read X" rows (LIB-SOC-005): tag similarity plus co-reading among people you know. */
function Because() {
  const t = useT();
  const q = useQuery({
    queryKey: ['library', 'recommendations'],
    queryFn: () => getRecommendations(8),
    staleTime: 5 * 60_000,
    retry: false,
  });
  const rows = q.data?.rows ?? [];
  if (rows.length === 0) return null;
  return (
    <>
      {rows.map((r) => (
        <section key={r.seed_id} className="lib-section" aria-label={t('library.home.because.title', { title: r.seed_title })}>
          <div className="lib-section-head">
            <h2 className="lib-h2">{t('library.home.because.title', { title: r.seed_title })}</h2>
          </div>
          <div className="lib-poster-row">
            {r.items.map((i) => (
              <SeriesCard key={i.series.id} series={i.series} />
            ))}
          </div>
        </section>
      ))}
    </>
  );
}

/* ---------------------------------------------------------- curiosity */

function Curiosity() {
  const t = useT();
  const q = useQuery({
    queryKey: keys.browse({ sort: 'followers', limit: 24 }),
    queryFn: () => browseSeries({ sort: 'followers', limit: 24 }),
  });
  const genres = useMemo(() => {
    const counts = new Map<string, number>();
    for (const s of q.data?.items ?? [])
      for (const g of s.genres) counts.set(g, (counts.get(g) ?? 0) + 1);
    const seen = [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([g]) => g);
    return [...new Set([...seen, ...DEFAULT_GENRES])].slice(0, 7);
  }, [q.data]);
  return (
    <section className="lib-section" aria-labelledby="lib-curiosity">
      <div className="lib-section-head">
        <h2 id="lib-curiosity" className="lib-h2">
          {t('library.home.curiosity.title')}
        </h2>
        <ArrowLink href="/library/browse" arrow="→">
          {t('library.home.curiosity.allGenres')}
        </ArrowLink>
      </div>
      <ul className="lib-genre-chips">
        {genres.map((g) => (
          <li key={g}>
            <Link to="/library/browse" search={{ genres: g }} className="lib-genre-chip">
              <Icon name="sparkle" size={16} />
              {label(g)}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* -------------------------------------------------------------- fresh */

function Fresh() {
  const t = useT();
  const q = useQuery({
    queryKey: keys.updates(false),
    queryFn: () => getUpdates({ limit: 4 }),
  });
  return (
    <section className="lib-section" aria-labelledby="lib-fresh">
      <div className="lib-section-head">
        <h2 id="lib-fresh" className="lib-h2">
          {t('library.home.fresh.title')}
        </h2>
        <ArrowLink href="/library/updates" arrow="→">
          {t('library.home.fresh.allUpdates')}
        </ArrowLink>
      </div>
      {q.isPending ? (
        <CardSkeletons n={3} />
      ) : (q.data?.items.length ?? 0) === 0 ? (
        <EmptyState
          title={t('library.home.fresh.emptyTitle')}
          description={t('library.home.fresh.emptyDescription')}
        />
      ) : (
        <ul className="lib-update-list">
          {q.data!.items.map((u) => (
            <li key={u.chapter_id} className="lib-update-row">
              <Link
                to="/library/s/$seriesId"
                params={{ seriesId: u.series_id }}
                className="lib-update-cover"
                aria-hidden
                tabIndex={-1}
              >
                {u.cover_url ? (
                  <img src={u.cover_url} alt="" />
                ) : (
                  <span>{u.series_title.charAt(0)}</span>
                )}
              </Link>
              <div className="lib-update-text">
                <Link
                  to="/library/s/$seriesId"
                  params={{ seriesId: u.series_id }}
                  className="lib-update-title"
                >
                  {u.series_title}
                </Link>
                {u.age_restricted && <AgeBadge />}
                <span className="lib-update-line">{updateLine(u)}</span>
              </div>
              <span className="lib-update-group">{u.release_group ?? ''}</span>
              <Link
                to="/library/r/$chapterId/{-$page}"
                params={{ chapterId: u.chapter_id }}
                className="lib-icon-btn"
                aria-label={t('library.home.fresh.read', {
                  title: u.series_title,
                  n: u.chapter_number,
                })}
              >
                <Icon name="play" size={20} />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/* -------------------------------------------------------- inspiration */

function Inspiration() {
  const t = useT();
  // Featured reading lists arrive with LIB-SOC-006; the card shows a fan of covers from the shelves.
  const q = useQuery({
    queryKey: keys.browse({ sort: 'newest', limit: 3 }),
    queryFn: () => browseSeries({ sort: 'newest', limit: 3 }),
  });
  // A list staff featured wins; until there is one the card shows the newest covers.
  const featured = useQuery({
    queryKey: socialKeys.featured(),
    queryFn: () => getFeaturedLists(1),
  });
  const list = featured.data?.items[0];
  const covers = list
    ? list.covers.slice(0, 3)
    : (q.data?.items ?? [])
        .filter((s) => s.cover_url)
        .slice(0, 3)
        .map((s) => s.cover_url!);
  return (
    <section className="lib-section" aria-labelledby="lib-inspo">
      <div className="lib-section-head">
        <h2 id="lib-inspo" className="lib-h2">
          {t('library.home.inspiration.title')}
        </h2>
        <ArrowLink href="/library/lists" arrow="→">
          {t('library.home.inspiration.moreLists')}
        </ArrowLink>
      </div>
      <div className="lib-list-card">
        <Eyebrow>{t('library.home.inspiration.eyebrow')}</Eyebrow>
        <h3 className="lib-list-title">
          {list ? (
            <Link to="/library/lists/$listId" params={{ listId: list.id }}>
              {list.name}
            </Link>
          ) : (
            t('library.home.inspiration.listTitle')
          )}
        </h3>
        <p className="lib-muted">
          {list
            ? list.description || t('library.home.inspiration.listBody')
            : t('library.home.inspiration.listBody')}
        </p>
        <div className="lib-fan" aria-hidden="true">
          {covers.map((src, i) => (
            <img key={src + i} src={src} alt="" style={{ ['--i' as string]: i }} />
          ))}
        </div>
      </div>
    </section>
  );
}

/* ----------------------------------------------------------- finished */

function Finished() {
  const t = useT();
  const q = useQuery({
    queryKey: keys.browse({ status: 'completed', sort: 'followers', limit: 5 }),
    queryFn: () => browseSeries({ status: 'completed', sort: 'followers', limit: 5 }),
  });
  return (
    <section className="lib-section" aria-labelledby="lib-finished">
      <div className="lib-section-head">
        <h2 id="lib-finished" className="lib-h2">
          {t('library.home.finished.title')}
        </h2>
        <ArrowLink href="/library/browse?status=completed" arrow="→">
          {t('library.home.finished.explore')}
        </ArrowLink>
      </div>
      <SeriesRow query={q} empty={t('library.home.finished.empty')} />
    </section>
  );
}

/* ------------------------------------------------------------- footer */

export function HomeFooter() {
  const t = useT();
  return (
    <footer className="lib-footer">
      <span className="lib-footer-brand">{t('library.home.footer.brand')}</span>
      <nav aria-label={t('library.home.footer.label')} className="lib-footer-links">
        <a href="/house-notes">{t('library.home.footer.about')}</a>
        <a href="/support">{t('library.home.footer.help')}</a>
        <a href="/support/leaderboards?tab=chapters">
          {t('library.home.footer.contributorBoards')}
        </a>
        <a href="/house-notes#image-credits">{t('library.home.footer.imageCredits')}</a>
        <a href="/house-notes#privacy">{t('library.home.footer.privacy')}</a>
        <a href="/house-notes#terms">{t('library.home.footer.terms')}</a>
      </nav>
    </footer>
  );
}
