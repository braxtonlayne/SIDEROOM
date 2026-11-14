import { isStaffLoaded } from './messages/store';

/**
 * SEC-AUD-187 (D-663): loads the staff catalog (`messages/en.staff.ts`, served from `/app/staff/` to appointed staff
 * only) for the staff door, and retries a failed load the way the rest catalog does (`startFullCatalog` in
 * `runtime.ts`, `catalogRetry.test.ts`): the promise settles after the second failure so the Admin app opens degraded
 * (message keys instead of text) rather than blank, and the tries go on in the background; when the catalog arrives
 * `markStaffLoaded` re-renders every `useT` subscriber and the keys turn into text.
 *
 * Only `admin/StaffDoor.tsx` imports this file, so it travels in the door's own lazy chunk and the first paint does not
 * carry it. `import('./messages/en.staff')` is allowed from member code because that module is a staff entry
 * (`scripts/lib/staff-graph.mjs`): it never counts as a member module.
 */
let staffCatalog: Promise<unknown> | undefined;
const loadStaffCatalog = (): Promise<unknown> => (staffCatalog ??= import('./messages/en.staff'));

/** Waits (ms) before each new try; same rhythm as the rest catalog. */
const RETRY_MS = [600, 2000, 5000, 15000];

/** Test hook: forget a cached (failed or finished) load. */
export function resetStaffCatalogForTests(): void {
  staffCatalog = undefined;
}

/**
 * Resolves once the staff catalog is registered, or after its second failed try. Never rejects.
 * `load` is injectable for tests.
 */
export function startStaffCatalog(load: () => Promise<unknown> = loadStaffCatalog): Promise<void> {
  if (isStaffLoaded()) return Promise.resolve();
  return new Promise<void>((settle) => {
    const attempt = (n: number) => {
      load().then(
        () => settle(),
        (e: unknown) => {
          staffCatalog = undefined;
          if (n >= 1) {
            if (n === 1) console.error('Could not load the staff message catalog', e);
            settle();
          }
          const wait = RETRY_MS[n];
          if (wait !== undefined) setTimeout(() => attempt(n + 1), wait);
        },
      );
    };
    attempt(0);
  });
}
