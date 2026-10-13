// Small shared pieces of the Library UI: formatting helpers, the series card, the 18+ badge.
import { PosterCard, Skeleton } from '@sideroom/ui';
import type { ReactNode } from 'react';
import type { Series, SeriesKind, SeriesStatus, UpdateItem } from './types';

export const KIND_LABEL: Record<SeriesKind, string> = {
  manga: 'Manga',
  manhwa: 'Manhwa',
  manhua: 'Manhua',
  webtoon: 'Webtoon',
  comic: 'Comic',
  novel: 'Novel',
  book: 'Book',
};

export const STATUS_LABEL: Record<SeriesStatus, string> = {
  ongoing: 'Ongoing',
  completed: 'Completed',
  hiatus: 'Hiatus',
  cancelled: 'Cancelled',
};

/** Server labels are lower-cased ("slice of life"); show them as sentence case. */
export const label = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

export const seriesHref = (id: string) => `/library/s/${id}`;
export function formatDate(ms: number | null | undefined): string {
  if (!ms) return '';
  return new Date(ms).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function timeAgo(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return 'just now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 30) return `${d} d ago`;
  return formatDate(ms);
}

/** "18+" marker for age-restricted titles (D-184: shown to everyone, only opening is gated). */
export function AgeBadge({ className = '' }: { className?: string }) {
  return (
    <span className={`lib-age-badge ${className}`.trim()} title="Age-restricted: 18 or older">
      <span aria-hidden="true">18+</span>
      <span className="lib-sr-only">Age-restricted, 18 or older</span>
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
  const sub =
    subtitle ??
    `${KIND_LABEL[series.kind]} · ${STATUS_LABEL[series.status]}${
      series.chapter_count ? ` · ${series.chapter_count} ch.` : ''
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
  `Chapter ${u.chapter_number} · ${LANG[u.language] ?? u.language.toUpperCase()}`;

const LANG: Record<string, string> = {
  en: 'English',
  ja: 'Japanese',
  ko: 'Korean',
  zh: 'Chinese',
  es: 'Spanish',
  fr: 'French',
  de: 'German',
  pt: 'Portuguese',
};
export const languageName = (code: string) => LANG[code] ?? code.toUpperCase();
