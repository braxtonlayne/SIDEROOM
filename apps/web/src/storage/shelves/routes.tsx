import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../../lib/lazyRoute';
import type { ShelfSection } from './api';

// SEC-AUD-144: the shelf pages (reader, metadata editor, review client) are not first-paint code.
const ShelfList = lazyNamed(() => import('./ShelfList'), 'ShelfList');
const ShelfItemPage = lazyNamed(() => import('./ShelfItemPage'), 'ShelfItemPage');

/**
 * `/{section}/shelves` and `/{section}/shelves/$itemId` under the Cinema or Library layout (BYOS-CAT-002). The
 * pages are small and share the section's chrome, and load on demand.
 */
export function createShelfRoutes(layout: AnyRoute, section: Exclude<ShelfSection, 'music'>) {
  const list = createRoute({
    getParentRoute: () => layout,
    path: 'shelves',
    component: function ShelvesRoute() {
      return <ShelfList section={section} />;
    },
  });
  const item = createRoute({
    getParentRoute: () => layout,
    path: 'shelves/$itemId',
    component: function ShelfItemRoute() {
      const { itemId } = item.useParams();
      return <ShelfItemPage key={itemId} section={section} id={itemId} />;
    },
  });
  return [list, item];
}
