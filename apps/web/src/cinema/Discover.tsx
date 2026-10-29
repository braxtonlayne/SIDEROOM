// Discover (CIN-DISC-001..009): hero "Tonight's feature", curated and generated rows, the animation banner.
import { RovingScope } from '../a11y/RovingNav';
import {
  ArrowLink,
  DisplayHeading,
  Eyebrow,
  FilterChips,
  GoldButton,
  Icon,
  Notice,
  OutlineButton,
  PosterRow,
} from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { t, useT } from '../i18n';
import { getDiscover, keys } from './api';
import { TrailerModal } from './TrailerModal';
import { ShelfEmptyState } from '../storage/ShelfEmpty';
import type { Row, Title, TrendingScope } from './types';
import {
  AgeBadge,
  CardSkeletons,
  formatClock,
  formatRuntime,
  isResumable,
  label,
  mergeMy,
  titleHref,
  TitleCard,
  useMy,
  useOpen,
  watchHref,
} from './ui';

export const HERO_INTERVAL_MS = 8000;

/** "Dune: Part Two" becomes ["Dune:", "Part Two."]: display serif first line, italic second line. */
export function splitTitle(name: string): [string, string | null] {
  const m = /^(.+?:)\s+(.+)$/.exec(name);
  if (!m) return [name, null];
  return [m[1]!, /[.!?]$/.test(m[2]!) ? m[2]! : `${m[2]}.`];
}

export function playLabel(title: Title): string {
  if (!isResumable(title)) return t('cinema.discover.play');
  return title.my?.position_s
    ? t('cinema.discover.resumeAt', { time: formatClock(title.my.position_s) })
    : t('cinema.discover.resume');
}

function Hero({ titles }: { titles: Title[] }) {
  const tr = useT();
  const open = useOpen();
  const [i, setI] = useState(0);
  const [paused, setPaused] = useState(false);
  const [trailer, setTrailer] = useState(false);
  const n = titles.length;
  const my = useMy();
  const current = titles[Math.min(i, n - 1)];
  const t = current ? mergeMy(current, my) : undefined;

  useEffect(() => {
    if (n < 2 || paused) return;
    if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(() => setI((x) => (x + 1) % n), HERO_INTERVAL_MS);
    return () => clearInterval(id);
  }, [n, paused]);

  if (!t) return null;
  const [first, second] = splitTitle(t.title);
  return (
    <section
      className="cin-hero"
      aria-roledescription="carousel"
      aria-label={tr('cinema.discover.tonightsFeatureLabel')}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      {t.backdrop_url && <img className="cin-hero-img" src={t.backdrop_url} alt="" />}
      <div className="cin-hero-shade" aria-hidden="true" />
      <div className="cin-hero-body" aria-live={paused ? 'polite' : 'off'}>
        <Eyebrow>{tr('cinema.discover.tonightsFeature')}</Eyebrow>
        <DisplayHeading as="h1" size="lg" accent={second ?? undefined}>
          {first}
        </DisplayHeading>
        <p className="cin-hero-meta">
          {t.year && <span>{t.year}</span>}
          {t.runtime_min ? <span>{formatRuntime(t.runtime_min)}</span> : null}
          {t.content_rating && <span className="cin-rating-badge">{t.content_rating}</span>}
          {t.genres[0] && <span>{label(t.genres[0])}</span>}
          {t.age_restricted && <AgeBadge />}
        </p>
        <p className="cin-hero-synopsis">{t.synopsis}</p>
        <div className="cin-hero-actions">
          <GoldButton onClick={() => void open(watchHref(t.id), t.age_restricted)}>
            <Icon name="play" size={16} /> {playLabel(t)}
          </GoldButton>
          <OutlineButton onClick={() => void open(titleHref(t.id), t.age_restricted)}>
            <Icon name="info" size={16} /> {tr('cinema.discover.moreInfo')}
          </OutlineButton>
          {(t.trailer_url || t.has_trailer) && (
            <OutlineButton
              onClick={() =>
                t.trailer_url
                  ? setTrailer(true)
                  : void open(`${watchHref(t.id)}?trailer=1`, t.age_restricted)
              }
            >
              {tr('cinema.discover.watchTrailer')}
            </OutlineButton>
          )}
        </div>
      </div>
      <div className="cin-hero-index">
        <span>
          {tr('cinema.discover.eveningSelection', {
            n: String(Math.min(i, n - 1) + 1).padStart(2, '0'),
          })}
        </span>
        {n > 1 && (
          <span
            className="cin-hero-dots"
            role="group"
            aria-label={tr('cinema.discover.chooseFeature')}
          >
            {titles.map((x, k) => (
              <button
                key={x.id}
                type="button"
                className="cin-hero-dot"
                aria-label={tr('cinema.discover.showTitle', { title: x.title })}
                aria-current={k === i || undefined}
                onClick={() => setI(k)}
              />
            ))}
          </span>
        )}
      </div>
      {t.trailer_url && (
        <TrailerModal
          open={trailer}
          onClose={() => setTrailer(false)}
          title={t.title}
          src={t.trailer_url}
        />
      )}
    </section>
  );
}

/** CIN-DISC-004: "Animation. Without limits." with a fanned poster collage. */
export function AnimationBanner({ posters }: { posters: Title[] }) {
  const tr = useT();
  const shown = posters.filter((p) => p.poster_url).slice(0, 4);
  return (
    <section className="cin-banner" aria-labelledby="cin-banner-h">
      <div className="cin-banner-copy">
        <Eyebrow>{tr('cinema.discover.bannerEyebrow')}</Eyebrow>
        <DisplayHeading
          as="h2"
          id="cin-banner-h"
          size="lg"
          accent={tr('cinema.discover.bannerAccent')}
        >
          {tr('cinema.discover.bannerHeading')}
        </DisplayHeading>
        <p className="cin-muted">{tr('cinema.discover.bannerBody')}</p>
        <ArrowLink href="/cinema/anime">{tr('cinema.discover.bannerLink')}</ArrowLink>
      </div>
      <div className="cin-banner-fan" aria-hidden="true">
        {shown.map((p, k) => (
          <img
            key={p.id}
            src={p.poster_url ?? ''}
            alt=""
            style={{ ['--i' as string]: k, ['--n' as string]: shown.length }}
          />
        ))}
      </div>
    </section>
  );
}

const SCOPES: { value: TrendingScope; label: string }[] = [
  { value: 'friends', label: 'cinema.discover.scopeFriends' },
  { value: 'known', label: 'cinema.discover.scopeKnown' },
  { value: 'global', label: 'cinema.discover.scopeGlobal' },
];

function RowView({
  row,
  scope,
  onScope,
}: {
  row: Row;
  scope: TrendingScope;
  onScope: (s: TrendingScope) => void;
}) {
  const tr = useT();
  if (row.titles.length === 0) return null;
  const heading = (
    <>
      {row.title}
      {row.accent && (
        <>
          {' '}
          <em>{row.accent}</em>
        </>
      )}
      {row.label && <span className="cin-row-label">{row.label}</span>}
    </>
  );
  const continuing = row.key === 'continue-watching';
  return (
    <div className="cin-row-wrap" data-row={row.key}>
      {row.key === 'trending' && (
        <FilterChips
          label={tr('cinema.discover.trendingScope')}
          className="cin-scope"
          options={SCOPES.map((o) => ({ ...o, label: tr(o.label) }))}
          value={scope}
          onValueChange={(v) => v && onScope(v as TrendingScope)}
        />
      )}
      <RovingScope selector="[data-poster-link]" orientation="horizontal">
        <PosterRow
          className="cin-row"
          title={heading}
          exploreHref={row.explore_href ?? undefined}
          aria-label={row.title}
        >
          {row.titles.map((t) => (
            <TitleCard
              key={t.id}
              title={t}
              subtitle={
                continuing && t.my?.position_s
                  ? tr('cinema.discover.resumeAt', { time: formatClock(t.my.position_s) })
                  : undefined
              }
            />
          ))}
        </PosterRow>
      </RovingScope>
    </div>
  );
}

export function Discover() {
  const tr = useT();
  const [scope, setScope] = useState<TrendingScope>('global');
  const q = useQuery({
    queryKey: keys.discover(scope),
    queryFn: () => getDiscover(scope),
    placeholderData: (prev) => prev,
  });

  if (q.isPending)
    return (
      <div className="cin-page">
        <CardSkeletons n={6} />
      </div>
    );
  if (q.isError)
    return (
      <div className="cin-page">
        <Notice variant="danger" title={tr('cinema.discover.errorTitle')}>
          {tr('cinema.discover.errorBody')}
        </Notice>
      </div>
    );

  const { hero, rows } = q.data;
  const visible = rows.filter((r) => r.titles.length > 0 && r.display !== 'hero');
  if (hero.length === 0 && visible.length === 0)
    return (
      <div className="cin-page">
        <ShelfEmptyState kind="cinema" />
      </div>
    );

  return (
    <>
      {/* The feature hero carries the page's h1; without one, keep a heading for the page (axe page-has-heading-one). */}
      {hero.length === 0 && <h1 className="cin-sr-only">{tr('cinema.shell.cinema')}</h1>}
      <Hero titles={hero} />
      <div className="cin-page cin-rows">
        {visible.map((r) => (
          <div key={r.key} className="cin-block">
            {r.display === 'banner' ? (
              <AnimationBanner posters={r.titles} />
            ) : (
              <RowView row={r} scope={scope} onScope={setScope} />
            )}
          </div>
        ))}
      </div>
    </>
  );
}
