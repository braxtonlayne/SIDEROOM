import { useParams, useSearch } from '@tanstack/react-router';
import { installJassub } from './jassubLoader';
import { parseWatchSearch } from './model';
import { WatchPage } from './Watch';

installJassub();

/** `/cinema/watch/$id?ep=&t=`: the full-screen player (outside the Cinema section chrome). */
export function WatchRoute() {
  const { id } = useParams({ strict: false }) as { id?: string };
  const search = parseWatchSearch(useSearch({ strict: false }) as Record<string, unknown>);
  if (!id) return null;
  return (
    <WatchPage
      titleId={String(id)}
      episodeId={search.episode}
      variantId={search.variant}
      trailer={search.trailer}
      startAt={search.t}
    />
  );
}
