import { Button, EmptyState, FilterChips, Notice, PosterCard, TextInput } from '@sideroom/ui';
import { keepPreviousData, useInfiniteQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useT } from '../../i18n';
import { listShelf, shelfKeys, type ShelfItem, type ShelfSection } from './api';
import { ShelfChip, useChipLabel } from './ShelfChip';
import './shelves.css';

const PAGE = 24;
const KINDS: Record<ShelfSection, string[]> = {
  cinema: ['movie', 'series', 'anime'],
  library: ['manga', 'comic', 'book'],
  music: [],
};

/** Where an item's page lives. */
export const shelfHref = (section: ShelfSection, id: string) =>
  section === 'music' ? `/music/shelves/${id}` : `/${section}/shelves/${id}`;

function ShelfCard({ item }: { item: ShelfItem }) {
  const t = useT();
  const chip = useChipLabel(item.source);
  const sub =
    item.section === 'music'
      ? item.subtitle
      : [t(`shelves.kind.${item.kind}` as 'shelves.kind.movie'), item.year ?? '']
          .filter(Boolean)
          .join(' · ');
  return (
    <li>
      <PosterCard
        title={item.title}
        subtitle={sub}
        meta={<ShelfChip source={item.source} />}
        src={item.poster_url ?? null}
        href={shelfHref(item.section, item.id)}
        aria-label={`${item.title}, ${chip}`}
      />
    </li>
  );
}

/**
 * The Shelves page of a section (BYOS-CAT-002): everything the viewer's mounts hold, with the source chip on each
 * card and the "Shelves" filter (one chip per reachable mount). The server decides what is listed
 * (`sr_storage::scopes`), so a mount the viewer cannot see never appears, not even as an empty chip.
 */
export function ShelfList({ section }: { section: ShelfSection }) {
  const t = useT();
  const [q, setQ] = useState('');
  const [kind, setKind] = useState<string | null>(null);
  const [mount, setMount] = useState<string | null>(null);
  const query = { q: q.trim() || undefined, kind: kind ?? undefined, mount: mount ?? undefined };
  const list = useInfiniteQuery({
    queryKey: shelfKeys.list(section, query),
    queryFn: ({ pageParam }) => listShelf(section, { ...query, limit: PAGE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (last, all) => {
      const shown = all.reduce((n, p) => n + p.items.length, 0);
      return shown < last.total ? shown : undefined;
    },
    retry: false,
    // Keep the cards and the Shelves chips on screen while a filter loads, so the chips do not blink away.
    placeholderData: keepPreviousData,
  });
  const pages = list.data?.pages ?? [];
  const items = pages.flatMap((p) => p.items);
  const sources = pages[0]?.sources ?? [];
  const total = pages[0]?.total ?? 0;
  const noFilter = !query.q && !query.kind && !query.mount;
  const kinds = KINDS[section];

  return (
    <section className="shv-page" aria-labelledby="shv-h">
      <header className="shv-head">
        <h1 id="shv-h">{t(`shelves.title.${section}` as 'shelves.title.cinema')}</h1>
        <p className="shv-muted">{t(`shelves.sub.${section}` as 'shelves.sub.cinema')}</p>
      </header>
      <div className="shv-tools">
        <TextInput
          label={t('shelves.search')}
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>
      {kinds.length > 0 && (
        <FilterChips
          label={t('shelves.filter.kind')}
          value={kind}
          onValueChange={setKind}
          options={kinds.map((k) => ({
            value: k,
            label: t(`shelves.kind.${k}` as 'shelves.kind.movie'),
          }))}
        />
      )}
      {sources.length > 1 && (
        <FilterChips
          label={t('shelves.filter.shelves')}
          value={mount}
          onValueChange={setMount}
          options={sources.map((s) => ({ value: s.mount_id, label: <SourceLabel source={s} /> }))}
        />
      )}
      {list.isPending && <p className="shv-muted">{t('shelves.loading')}</p>}
      {list.isError && <Notice variant="danger">{t('shelves.error')}</Notice>}
      {list.isSuccess && items.length === 0 && (
        <EmptyState
          title={noFilter ? t('shelves.empty.title') : t('shelves.noMatch')}
          description={noFilter ? t('shelves.empty.body') : undefined}
        />
      )}
      {items.length > 0 && (
        <>
          <h2 className="shv-visually-hidden">{t('shelves.results')}</h2>
          <p className="shv-muted" aria-live="polite">
            {t('shelves.count', { shown: items.length, total })}
          </p>
          <ul className="shv-grid">
            {items.map((it) => (
              <ShelfCard key={it.id} item={it} />
            ))}
          </ul>
          {list.hasNextPage && (
            <Button
              variant="secondary"
              loading={list.isFetchingNextPage}
              onClick={() => void list.fetchNextPage()}
            >
              {t('shelves.more')}
            </Button>
          )}
        </>
      )}
    </section>
  );
}

function SourceLabel({ source }: { source: ShelfItem['source'] }) {
  return <>{useChipLabel(source)}</>;
}
