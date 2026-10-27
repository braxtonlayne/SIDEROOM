import { EmptyState } from '@sideroom/ui';
import { useT } from '../i18n';
import './shelves/shelves.css';

export type ShelfKind = 'cinema' | 'library' | 'music';

/**
 * The empty-shelf state shared by Cinema, Library and Music (BYOS-NAV-001): nothing is reachable, so say how
 * to get something (mount a bucket, or ask a friend to share).
 */
export function ShelfEmptyState({ kind }: { kind: ShelfKind }) {
  const t = useT();
  const what =
    kind === 'cinema'
      ? t('shelves.nothing.cinema')
      : kind === 'library'
        ? t('shelves.nothing.library')
        : t('shelves.nothing.music');
  return (
    <EmptyState
      data-testid={`shelf-empty-${kind}`}
      title={t('shelves.nothing.title')}
      description={`${what} ${t('shelves.nothing.hint')}`}
      action={
        <span className="shv-empty-links">
          <a href="/settings/user/storage">{t('shelves.nothing.mount')}</a>
          <a href="/channels/@me">{t('shelves.nothing.friends')}</a>
        </span>
      }
    />
  );
}
