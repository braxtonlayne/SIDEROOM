import { useParams, useSearch } from '@tanstack/react-router';
import { parsePartySearch } from './routes';
import { WatchPartyPage } from './WatchPartyPage';

/** `/cinema/party/$id?code=`: the theatre, outside the Cinema section chrome. */
export function PartyRoute() {
  const { id } = useParams({ strict: false }) as { id?: string };
  const { code } = parsePartySearch(useSearch({ strict: false }) as Record<string, unknown>);
  if (!id) return null;
  return <WatchPartyPage id={String(id)} code={code} />;
}
