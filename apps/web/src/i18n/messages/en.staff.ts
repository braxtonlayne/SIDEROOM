import { markStaffLoaded, registerFragments } from './store';

/**
 * The staff half of the English catalog (SEC-AUD-187, D-663): the fragments in `./en-staff/`, whose keys only
 * staff-only modules name (`node scripts/staff-keys.mjs --write` puts them there; docs/dev/entrance-isolation.md
 * "Staff code"). This module is a STAFF ENTRY of the build (`scripts/lib/staff-graph.mjs`), so Vite puts it, and the
 * fragments it globs, under `/app/staff/`, which the server serves only to appointed staff. The only importer is the
 * staff door's loader (`../staffCatalog.ts`), by a dynamic import, awaited with the Admin app before it renders.
 * Tests register it in `test/setup.ts`. It does not call `markRestLoaded`: the rest catalog is another module.
 */
const staff = import.meta.glob<Record<string, string>>('./en-staff/*.ts', {
  eager: true,
  import: 'default',
});
registerFragments(staff);
markStaffLoaded();

export const staffLoaded = true;
