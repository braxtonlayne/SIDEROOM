import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { ShelfItemPage } from './ShelfItemPage';
import { ShelfList } from './ShelfList';
import type { ShelfSection } from './api';

/**
 * `/{section}/shelves` and `/{section}/shelves/$itemId` under the Cinema or Library layout (BYOS-CAT-002). The
 * pages are small and share the section's chrome, so they are not split into their own chunks.
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
