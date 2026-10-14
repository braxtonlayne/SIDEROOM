// Shared Cinema pieces: formatting, the 18+ badge, the age-gated opener, the title poster card (CIN-DISC-009).
import { Icon, PosterCard, Skeleton } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from '@tanstack/react-router';
import { useState, type MouseEvent, type ReactNode } from 'react';
import { t, useT } from '../i18n';
import { useAgeGate } from '../lib/ageGate';
import { getMyCinema, keys, setInList, setWatched } from './api';
import type { MyCinema, Title, TitleKind } from './types';

export const KIND_LABEL: Record<TitleKind, string> = {
  movie: 'cinema.ui.kindFilm',
  series: 'cinema.ui.kindSeries',
  anime: 'cinema.ui.kindAnime',
};

export const titleHref = (id: string) => `/cinema/t/${id}`;
export const watchHref = (id: string) => `/cinema/watch/${id}`;

/** Server genres are lower-case ("science fiction"); show sentence case. */
export const label = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

export function formatRuntime(min: number | null | undefined): string {
  if (!min || min <= 0) return '';
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h ? `${h}h ${String(m).padStart(2, '0')}m` : `${m}m`;
}

/** "42:10" or "1:02:03". */
export function formatClock(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${r}` : `${m}:${r}`;
}

/** "2024 · Film". */
export const yearLine = (ref: Pick<Title, 'year' | 'kind'>) =>
  [ref.year, t(KIND_LABEL[ref.kind])].filter(Boolean).join(' · ');

/** True when the title has been started but not finished: the Play button reads "Resume". */
export const isResumable = (t: Title) => {
  const p = t.my?.progress_percent ?? 0;
  return p > 0 && p < 90 && !t.my?.watched;
};

/** The caller's saved list and resume points (shared by every card on the page). */
export function useMy(): MyCinema | undefined {
  return useQuery({ queryKey: keys.my(), queryFn: getMyCinema, staleTime: 30_000 }).data;
}

/** Overlay the caller's own state onto a catalog title. */
export function mergeMy(t: Title, my: MyCinema | undefined): Title {
  if (!my) return t;
  const r = my.resume.get(t.id);
  return {
    ...t,
    my: {
      ...t.my,
      in_list: my.inList.has(t.id),
      ...(r
        ? {
            progress_percent: r.percent,
            position_s: r.position_s ?? null,
            episode_id: r.episode_id ?? null,
          }
        : {}),
    },
  };
}

export function AgeBadge({ className = '' }: { className?: string }) {
  const tr = useT();
  return (
    <span className={`cin-age-badge ${className}`.trim()} title={tr('cinema.ui.ageTitle')}>
      <span aria-hidden="true">18+</span>
      <span className="cin-sr-only">{tr('cinema.ui.ageSr')}</span>
    </span>
  );
}

/**
 * Opens a path in the app. For an age-restricted title the shared age gate runs first (D-184): a no or a
 * dismissed dialog leaves the user where they are.
 */
export function useOpen() {
  const router = useRouter();
  const { ensureAdult } = useAgeGate();
  return async (href: string, restricted = false) => {
    if (restricted && !(await ensureAdult())) return;
    router.history.push(href);
  };
}

export function TitleCard({
  title: base,
  tag,
  subtitle,
  actions = true,
}: {
  title: Title;
  tag?: ReactNode;
  subtitle?: ReactNode;
  actions?: boolean;
}) {
  const tr = useT();
  const open = useOpen();
  const my = useMy();
  const title = mergeMy(base, my);
  const [seenLocal, setSeenLocal] = useState<boolean | null>(null);
  const qc = useQueryClient();
  const done = () => void qc.invalidateQueries({ queryKey: ['cinema'] });
  const list = useMutation({
    mutationFn: (on: boolean) => setInList(title.id, on),
    onSuccess: done,
  });
  const seen = useMutation({
    mutationFn: (w: boolean) => setWatched(title.id, w),
    onSuccess: (_d, w) => {
      setSeenLocal(w);
      done();
    },
  });
  const inList = title.my?.in_list ?? false;
  const watched = seenLocal ?? title.my?.watched ?? false;

  // Restricted titles ask first. The anchors inside the card are plain <a>, so intercept at the wrapper.
  const onClickCapture = (e: MouseEvent<HTMLElement>) => {
    if (!title.age_restricted) return;
    const a = (e.target as HTMLElement).closest?.('a');
    const href = a?.getAttribute('href');
    if (!a || !href) return;
    e.preventDefault();
    e.stopPropagation();
    void open(href, true);
  };

  return (
    <div
      className="cin-card"
      data-kind={title.kind}
      data-restricted={title.age_restricted || undefined}
      onClickCapture={onClickCapture}
    >
      <PosterCard
        title={title.title}
        subtitle={subtitle ?? yearLine(title)}
        src={title.poster_url}
        href={titleHref(title.id)}
        tag={tag ?? (title.kind === 'anime' ? tr('cinema.ui.kindAnime') : undefined)}
        progress={isResumable(title) ? (title.my?.progress_percent ?? 0) : undefined}
        actions={
          actions ? (
            <>
              <a
                className="cin-qa"
                href={watchHref(title.id)}
                aria-label={tr('cinema.ui.play', { title: title.title })}
                title={tr('cinema.ui.playShort')}
              >
                <Icon name="play" size={16} />
              </a>
              <button
                type="button"
                className="cin-qa"
                aria-label={
                  inList
                    ? tr('cinema.ui.removeFromList', { title: title.title })
                    : tr('cinema.ui.addToList', { title: title.title })
                }
                aria-pressed={inList}
                title={tr('cinema.ui.myList')}
                onClick={() => list.mutate(!inList)}
              >
                <Icon name={inList ? 'check' : 'plus'} size={16} />
              </button>
              <button
                type="button"
                className="cin-qa cin-qa-extra"
                aria-label={
                  watched
                    ? tr('cinema.ui.markUnwatched', { title: title.title })
                    : tr('cinema.ui.markWatched', { title: title.title })
                }
                aria-pressed={watched}
                title={tr('cinema.ui.watched')}
                onClick={() => seen.mutate(!watched)}
              >
                <Icon name="eye" size={16} />
              </button>
              <a
                className="cin-qa cin-qa-extra"
                href={`${titleHref(title.id)}#rate`}
                aria-label={tr('cinema.ui.rate', { title: title.title })}
                title={tr('cinema.ui.rateShort')}
              >
                <Icon name="star" size={16} />
              </a>
            </>
          ) : undefined
        }
      />
      {title.age_restricted && <AgeBadge className="cin-age-badge-card" />}
    </div>
  );
}

export function CardSkeletons({ n = 6 }: { n?: number }) {
  return (
    <div className="cin-skeletons" aria-hidden="true">
      {Array.from({ length: n }, (_, i) => (
        <Skeleton key={i} height={240} />
      ))}
    </div>
  );
}
