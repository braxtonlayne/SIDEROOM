import { Button } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useState, type MouseEvent, type ReactNode } from 'react';
import { getTitle } from '../../cinema/api';
import type { Title } from '../../cinema/types';
import { useT, type MessageKey } from '../../i18n';
import { getSeries } from '../../library/api';
import type { Series } from '../../library/types';

const StartPartyDialog = lazy(() =>
  import('../../watchparty/StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

type Loaded<T> = { state: 'loading' } | { state: 'error' } | { state: 'ready'; value: T };

/** One request per id for the whole session: a title linked many times in a channel loads once. */
function memo<T>(load: (id: string) => Promise<T>) {
  const cache = new Map<string, Promise<T>>();
  return {
    get(id: string): Promise<T> {
      let p = cache.get(id);
      if (!p) {
        p = load(id);
        cache.set(id, p);
        p.catch(() => cache.delete(id));
      }
      return p;
    },
    reset: () => cache.clear(),
  };
}
const titles = memo(async (id: string) => (await getTitle(id)).title);
const seriesCache = memo(async (id: string) => (await getSeries(id)).series);
export const resetRichCards = () => {
  titles.reset();
  seriesCache.reset();
};

function useLoaded<T>(source: { get: (id: string) => Promise<T> }, id: string): Loaded<T> {
  const [s, setS] = useState<Loaded<T>>({ state: 'loading' });
  useEffect(() => {
    let live = true;
    setS({ state: 'loading' });
    source.get(id).then(
      (value) => live && setS({ state: 'ready', value }),
      () => live && setS({ state: 'error' }),
    );
    return () => {
      live = false;
    };
  }, [source, id]);
  return s;
}

/** An in-app link: client-side navigation when a router is mounted, a plain anchor otherwise. */
function AppLink({
  to,
  className,
  children,
}: {
  to: string;
  className?: string;
  children: ReactNode;
}) {
  const router = useRouter({ warn: false }) as ReturnType<typeof useRouter> | undefined;
  const go = (e: MouseEvent<HTMLAnchorElement>) => {
    if (!router?.history || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.history.push(to);
  };
  return (
    <a className={className} href={to} onClick={go}>
      {children}
    </a>
  );
}

function CardShell({
  img,
  children,
  actions,
  label,
}: {
  img: ReactNode;
  children: ReactNode;
  actions: ReactNode;
  label: string;
}) {
  return (
    <span className="lng-rich" role="group" aria-label={label}>
      {img}
      <span className="lng-rich-body">
        {children}
        <span className="lng-rich-actions">{actions}</span>
      </span>
    </span>
  );
}

function Placeholder({ kind }: { kind: 'cinema' | 'library' }) {
  const t = useT();
  return (
    <span className="lng-rich lng-rich-skeleton" aria-busy="true">
      {kind === 'cinema' ? t('lounge.richCard.loadingTitle') : t('lounge.richCard.loadingSeries')}
    </span>
  );
}

function Fallback({ kind }: { kind: 'cinema' | 'library' }) {
  const t = useT();
  return (
    <span className="sr-md-mention">
      {kind === 'cinema' ? t('lounge.richCard.unknownTitle') : t('lounge.richCard.unknownSeries')}
    </span>
  );
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function titleMeta(title: Title, t: ReturnType<typeof useT>): string {
  const parts: string[] = [t(`lounge.richCard.kind${cap(title.kind)}` as MessageKey)];
  if (title.year) parts.push(String(title.year));
  if (title.runtime_min && title.kind === 'movie')
    parts.push(t('lounge.richCard.minutes', { n: title.runtime_min }));
  if (title.season_count && title.kind !== 'movie')
    parts.push(t('lounge.richCard.seasons', { n: title.season_count }));
  return parts.join(' · ');
}

/** `<cinema:ID>` in a message: poster, title, meta, Watch and Watch together (LNG-MSGUI-018). */
export function CinemaCard({ id }: { id: string }) {
  const t = useT();
  const res = useLoaded(titles, id);
  const [party, setParty] = useState(false);
  if (res.state === 'loading') return <Placeholder kind="cinema" />;
  if (res.state === 'error') return <Fallback kind="cinema" />;
  const title = res.value;
  const hide = title.age_restricted;
  return (
    <>
      <CardShell
        label={title.title}
        img={
          title.poster_url && !hide ? (
            <img className="lng-rich-img" src={title.poster_url} alt="" loading="lazy" />
          ) : (
            <span className="lng-rich-img lng-rich-noimg" aria-hidden="true" />
          )
        }
        actions={
          <>
            <AppLink to={`/cinema/t/${encodeURIComponent(title.id)}`} className="lng-rich-btn">
              {t('lounge.richCard.watch')}
            </AppLink>
            <Button size="sm" variant="secondary" onClick={() => setParty(true)}>
              {t('lounge.richCard.watchTogether')}
            </Button>
          </>
        }
      >
        <strong className="lng-rich-title">{title.title}</strong>
        <span className="lng-rich-meta">
          {titleMeta(title, t)}
          {hide && <span className="lng-rich-badge">{t('lounge.richCard.adult')}</span>}
        </span>
        {!hide && title.synopsis && <span className="lng-rich-text">{title.synopsis}</span>}
      </CardShell>
      {party && (
        <Suspense fallback={null}>
          <StartPartyDialog
            open
            onClose={() => setParty(false)}
            title={{ titleId: title.id, label: title.title }}
          />
        </Suspense>
      )}
    </>
  );
}

function seriesMeta(s: Series, t: ReturnType<typeof useT>): string {
  const parts: string[] = [t(`lounge.richCard.series${cap(s.kind)}` as MessageKey)];
  if (s.year) parts.push(String(s.year));
  parts.push(t('lounge.richCard.chapters', { n: s.chapter_count }));
  return parts.join(' · ');
}

/** `<library:ID>` in a message: cover, title, meta and Read. */
export function LibraryCard({ id }: { id: string }) {
  const t = useT();
  const res = useLoaded(seriesCache, id);
  if (res.state === 'loading') return <Placeholder kind="library" />;
  if (res.state === 'error') return <Fallback kind="library" />;
  const s = res.value;
  const hide = s.age_restricted;
  return (
    <CardShell
      label={s.title}
      img={
        s.cover_url && !hide ? (
          <img className="lng-rich-img" src={s.cover_url} alt="" loading="lazy" />
        ) : (
          <span className="lng-rich-img lng-rich-noimg" aria-hidden="true" />
        )
      }
      actions={
        <AppLink to={`/library/s/${encodeURIComponent(s.id)}`} className="lng-rich-btn">
          {t('lounge.richCard.read')}
        </AppLink>
      }
    >
      <strong className="lng-rich-title">{s.title}</strong>
      <span className="lng-rich-meta">
        {seriesMeta(s, t)}
        {hide && <span className="lng-rich-badge">{t('lounge.richCard.adult')}</span>}
      </span>
      {!hide && s.synopsis && <span className="lng-rich-text">{s.synopsis}</span>}
    </CardShell>
  );
}

/** Markdown resolvers for `<cinema:ID>` and `<library:ID>`. */
export const richCardResolvers = {
  cinema: (id: string) => <CinemaCard id={id} />,
  library: (id: string) => <LibraryCard id={id} />,
};
