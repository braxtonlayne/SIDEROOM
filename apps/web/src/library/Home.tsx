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
import { browseSeries, getChapters, getUpdates, keys } from './api';
import { progressApi, progressLine } from './progress';
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
  return (
    <div className="lib-home">
      <div className="lib-home-head">
        <DisplayHeading size="lg">Find your next good story.</DisplayHeading>
        <p className="lib-sub">
          A familiar favorite or something unexpected. Make yourself at home.
        </p>
      </div>
      <Spotlight />
      <PickUp />
      <Trending />
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
    <section className="lib-spotlight" aria-roledescription="carousel" aria-label="The spotlight">
      {s.cover_url && (
        <div className="lib-spotlight-art" aria-hidden="true">
          <img src={s.cover_url} alt="" />
        </div>
      )}
      <div className="lib-spotlight-body" aria-live="polite">
        <Eyebrow>The spotlight</Eyebrow>
        <DisplayHeading as="h2" size="md">
          {s.title}
        </DisplayHeading>
        <p className="lib-spotlight-meta">
          <span>
            {KIND_LABEL[s.kind]} · {STATUS_LABEL[s.status]}
          </span>
          {s.age_restricted && <AgeBadge />}
        </p>
        <ul className="lib-chip-list" aria-label="Genres">
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
              Start reading
            </Link>
          ) : null}
          <Link to="/library/s/$seriesId" params={{ seriesId: s.id }} className="lib-btn">
            View details
          </Link>
        </div>
      </div>
      <p className="lib-spotlight-caption">Recently updated on the shelves</p>
      {slides.length > 1 && (
        <div className="lib-dots" role="group" aria-label="Choose spotlight">
          {slides.map((x, i) => (
            <button
              key={x.id}
              type="button"
              className="lib-dot"
              aria-label={`Show ${x.title}`}
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
          Pick up where you left off
        </h2>
        <ArrowLink href="/library/history" arrow="→">
          Reading history
        </ArrowLink>
      </div>
      <ul className="lib-pickup-list">
        {items.map((i) => (
          <li key={i.chapterId} className="lib-pickup-card">
            <Link
              to="/library/r/$chapterId/{-$page}"
              params={{ chapterId: i.chapterId, page: String(i.pageIndex + 1) }}
              className="lib-pickup-link"
              aria-label={`Continue ${i.seriesTitle}`}
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
                <ProgressBar value={i.percent} label={`${i.seriesTitle} progress`} size={3} />
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
  const [scope, setScope] = useState('global');
  const q = useQuery({
    queryKey: keys.browse({ sort: 'followers', limit: 5 }),
    queryFn: () => browseSeries({ sort: 'followers', limit: 5 }),
  });
  return (
    <section className="lib-section" aria-labelledby="lib-trending">
      <div className="lib-section-head">
        <div className="lib-section-title">
          <h2 id="lib-trending" className="lib-h2">
            Trending this week
          </h2>
          <span className="lib-section-note">The stories everyone&apos;s talking about</span>
        </div>
        <SegmentedControl
          label="Trending scope"
          size="sm"
          value={scope}
          onValueChange={setScope}
          options={[
            // Scoped trending needs the social graph and the trending job (LIB-SOC-002, LIB-SOC-004).
            { value: 'friends', label: 'Friends', disabled: true },
            { value: 'known', label: 'People you know', disabled: true },
            { value: 'global', label: 'Global' },
          ]}
        />
        <ArrowLink href="/library/browse?sort=followers" arrow="→">
          See rankings
        </ArrowLink>
      </div>
      <SeriesRow query={q} empty="Nothing is trending yet." tagFirst />
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
  if (query.isPending) return <CardSkeletons />;
  if (query.isError) return <p className="lib-muted">We could not load this right now.</p>;
  const items = query.data?.items ?? [];
  if (items.length === 0) return <p className="lib-muted">{empty}</p>;
  return (
    <div className="lib-poster-row">
      {items.map((s, i) => (
        <SeriesCard key={s.id} series={s} tag={tagFirst && i === 0 ? '#1 TRENDING' : undefined} />
      ))}
    </div>
  );
}

/* ---------------------------------------------------------- curiosity */

function Curiosity() {
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
          Follow your curiosity
        </h2>
        <ArrowLink href="/library/browse" arrow="→">
          All genres
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
  const q = useQuery({
    queryKey: keys.updates(false),
    queryFn: () => getUpdates({ limit: 4 }),
  });
  return (
    <section className="lib-section" aria-labelledby="lib-fresh">
      <div className="lib-section-head">
        <h2 id="lib-fresh" className="lib-h2">
          Fresh off the press
        </h2>
        <ArrowLink href="/library/updates" arrow="→">
          All updates
        </ArrowLink>
      </div>
      {q.isPending ? (
        <CardSkeletons n={3} />
      ) : (q.data?.items.length ?? 0) === 0 ? (
        <EmptyState title="No chapters yet" description="New releases will show up here." />
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
                aria-label={`Read ${u.series_title} chapter ${u.chapter_number}`}
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
  // Featured reading lists arrive with LIB-SOC-006; the card shows a fan of covers from the shelves.
  const q = useQuery({
    queryKey: keys.browse({ sort: 'newest', limit: 3 }),
    queryFn: () => browseSeries({ sort: 'newest', limit: 3 }),
  });
  const covers = (q.data?.items ?? []).filter((s) => s.cover_url).slice(0, 3);
  return (
    <section className="lib-section" aria-labelledby="lib-inspo">
      <div className="lib-section-head">
        <h2 id="lib-inspo" className="lib-h2">
          A little inspiration
        </h2>
        <ArrowLink href="/library/lists" arrow="→">
          More lists
        </ArrowLink>
      </div>
      <div className="lib-list-card">
        <Eyebrow>A reading list</Eyebrow>
        <h3 className="lib-list-title">For a slow Sunday</h3>
        <p className="lib-muted">Gentle worlds, small moments, stories that stay with you.</p>
        <div className="lib-fan" aria-hidden="true">
          {covers.map((s, i) => (
            <img key={s.id} src={s.cover_url!} alt="" style={{ ['--i' as string]: i }} />
          ))}
        </div>
      </div>
    </section>
  );
}

/* ----------------------------------------------------------- finished */

function Finished() {
  const q = useQuery({
    queryKey: keys.browse({ status: 'completed', sort: 'followers', limit: 5 }),
    queryFn: () => browseSeries({ status: 'completed', sort: 'followers', limit: 5 }),
  });
  return (
    <section className="lib-section" aria-labelledby="lib-finished">
      <div className="lib-section-head">
        <h2 id="lib-finished" className="lib-h2">
          Finished stories, new beginnings
        </h2>
        <ArrowLink href="/library/browse?status=completed" arrow="→">
          Explore completed
        </ArrowLink>
      </div>
      <SeriesRow query={q} empty="No finished series yet." />
    </section>
  );
}

/* ------------------------------------------------------------- footer */

export function HomeFooter() {
  return (
    <footer className="lib-footer">
      <span className="lib-footer-brand">SIDE ROOM / THE LIBRARY</span>
      <nav aria-label="Library footer" className="lib-footer-links">
        <a href="/house-notes">About</a>
        <a href="/support">Help</a>
        <a href="/house-notes#image-credits">Image credits</a>
        <a href="/house-notes#privacy">Privacy</a>
        <a href="/house-notes#terms">Terms</a>
      </nav>
    </footer>
  );
}
