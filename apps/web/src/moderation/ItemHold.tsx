// What the Library reader and the Cinema player say about an item a takedown notice touches (MOD-TKD-011;
// docs/specs/moderation-compliance.md section 5). The server answers only the state: `held` (frozen for evidence
// while staff look at a notice; still reachable by those who can open it) or `taken_down` (made unavailable).
import { Notice } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useT } from '../i18n';
import { itemStatus, type ItemHoldKind, type ItemHoldState } from './api';
import './moderation.css';

/** The hold state of a library or cinema item; `undefined` while it loads or if the question failed. */
export function useItemHoldState(
  kind: ItemHoldKind,
  id: string | null | undefined,
): ItemHoldState | undefined {
  const q = useQuery({
    queryKey: ['moderation', 'item-status', kind, id],
    queryFn: () => itemStatus(kind, id!),
    enabled: !!id,
    retry: false,
    staleTime: 60_000,
    refetchOnWindowFocus: false,
  });
  return q.data?.state;
}

/** A banner for an item that is under a hold; nothing for any other state. */
export function ItemHoldBanner({ state }: { state: ItemHoldState | undefined }) {
  const t = useT();
  if (state !== 'held') return null;
  return (
    <div className="mod-hold-banner">
      <Notice variant="warning" role="status" title={t('moderation.itemHold.heldTitle')}>
        {t('moderation.itemHold.heldBody')}
      </Notice>
    </div>
  );
}
