// Small shared pieces of the Library UI: formatting helpers, the series card, the 18+ badge.
import { PosterCard, Skeleton } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { formatDate as formatIntlDate, t, useT } from '../i18n';
import type { Series, SeriesKind, SeriesStatus, UpdateItem } from './types';

/** Getters resolve the label at access time (never at module load: the locale is not known yet). */
export const KIND_LABEL: Record<SeriesKind, string> = {
  get manga() {
    return t('library.ui.kind.manga');
  },
  get manhwa() {
    return t('library.ui.kind.manhwa');
  },
  get manhua() {
    return t('library.ui.kind.manhua');
  },
  get webtoon() {
    return t('library.ui.kind.webtoon');
  },
  get comic() {
    return t('library.ui.kind.comic');
  },
  get novel() {
    return t('library.ui.kind.novel');
  },
  get book() {
    return t('library.ui.kind.book');
  },
};

export const STATUS_LABEL: Record<SeriesStatus, string> = {
  get ongoing() {
    return t('library.ui.status.ongoing');
  },
  get completed() {
    return t('library.ui.status.completed');
  },
  get hiatus() {
    return t('library.ui.status.hiatus');
  },
  get cancelled() {
    return t('library.ui.status.cancelled');
  },
};

/** Server labels are lower-cased ("slice of life"); show them as sentence case. */
export const label = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

export const seriesHref = (id: string) => `/library/s/${id}`;
export function formatDate(ms: number | null | undefined): string {
  if (!ms) return '';
  return formatIntlDate(ms, { year: 'numeric', month: 'short', day: 'numeric' });
}

export function timeAgo(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return t('library.ui.justNow');
  const m = Math.round(s / 60);
  if (m < 60) return t('library.ui.minAgo', { m });
  const h = Math.round(m / 60);
  if (h < 24) return t('library.ui.hourAgo', { h });
  const d = Math.round(h / 24);
  if (d < 30) return t('library.ui.dayAgo', { d });
  return formatDate(ms);
}

/** "18+" marker for age-restricted titles (D-184: shown to everyone, only opening is gated). */
export function AgeBadge({ className = '' }: { className?: string }) {
  const t = useT();
  return (
    <span className={`lib-age-badge ${className}`.trim()} title={t('library.ui.ageBadgeTitle')}>
      <span aria-hidden="true">18+</span>
      <span className="lib-sr-only">{t('library.ui.ageBadgeSr')}</span>
    </span>
  );
}

export interface SeriesCardProps {
  series: Series;
  /** Corner tag, e.g. "#1 TRENDING". */
  tag?: ReactNode;
  subtitle?: ReactNode;
}

/** A poster for one series. Genres go in the third line; restricted titles carry the 18+ badge. */
export function SeriesCard({ series, tag, subtitle }: SeriesCardProps) {
  const t = useT();
  const sub =
    subtitle ??
    `${KIND_LABEL[series.kind]} · ${STATUS_LABEL[series.status]}${
      series.chapter_count ? t('library.ui.chapterCountShort', { n: series.chapter_count }) : ''
    }`;
  return (
    <div className="lib-card" data-restricted={series.age_restricted || undefined}>
      <PosterCard
        title={series.title}
        subtitle={sub}
        meta={series.genres.slice(0, 2).map(label).join(' · ') || undefined}
        src={series.cover_url}
        href={seriesHref(series.id)}
        tag={tag}
      />
      {series.age_restricted && <AgeBadge className="lib-age-badge-card" />}
    </div>
  );
}

export function CardSkeletons({ n = 5 }: { n?: number }) {
  return (
    <div className="lib-row-skeleton" aria-hidden="true">
      {Array.from({ length: n }, (_, i) => (
        <Skeleton key={i} height={240} />
      ))}
    </div>
  );
}

/** "Chapter 138 · English" for an updates row. */
export const updateLine = (u: Pick<UpdateItem, 'chapter_number' | 'language'>) =>
  t('library.ui.updateLine', {
    n: u.chapter_number,
    lang: LANG[u.language] ?? u.language.toUpperCase(),
  });

const LANG: Record<string, string> = {
  get en() {
    return t('library.ui.lang.en');
  },
  get ja() {
    return t('library.ui.lang.ja');
  },
  get ko() {
    return t('library.ui.lang.ko');
  },
  get zh() {
    return t('library.ui.lang.zh');
  },
  get es() {
    return t('library.ui.lang.es');
  },
  get fr() {
    return t('library.ui.lang.fr');
  },
  get de() {
    return t('library.ui.lang.de');
  },
  get pt() {
    return t('library.ui.lang.pt');
  },
};
export const languageName = (code: string) => LANG[code] ?? code.toUpperCase();
