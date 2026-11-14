import { writeFileSync } from 'node:fs';
import type { Plugin } from 'vite';
import { STAFF_ENTRIES, sortModules } from '../../../scripts/lib/staff-graph.mjs';

/**
 * SEC-AUD-170 (D-651, docs/dev/entrance-isolation.md "Staff code"): the app build puts every chunk that only a staff
 * entry reaches under `dist/app/staff/`, named by its hash alone. `sr-server` serves that directory only to accounts
 * with a staff appointment, so a member is never sent the staff tools.
 *
 * Which chunk is staff is COMPUTED from the module graph, never listed by file:
 *
 * - the MEMBER SET is every module reached from the app entry through static and dynamic imports, without following a
 *   dynamic import whose target is a staff entry;
 * - every other module of the build is STAFF-ONLY. A module the member code also imports (`admin/reauth.ts`,
 *   `admin/api.ts`, ...) is in the member set and stays where it is: that is correct.
 * - a chunk none of whose modules is in the member set is a staff chunk.
 *
 * The walk and the entry list live in `scripts/lib/staff-graph.mjs`, shared with the key audit
 * (`scripts/staff-keys.mjs`, D-663), so "staff-only" has one definition. Two entries: the Admin app, and the staff
 * message catalog (`src/i18n/messages/en.staff.ts`, which the door loads beside the Admin app).
 *
 * HOW TO ADD A STAFF ENTRY: add its path (relative to `apps/web`, forward slashes) to `STAFF_ENTRIES`, reach it from
 * member code ONLY through a dynamic `import()` made by a small door that asks the server first (see
 * `src/admin/StaffDoor.tsx`), and run the build and `node scripts/check-staff-delivery.mjs`. A static import of a staff
 * entry from member code fails the build, naming the importer.
 */
export { STAFF_ENTRIES, sortModules };
export type { ModuleNode } from '../../../scripts/lib/staff-graph.mjs';

const slash = (p: string) => p.replaceAll('\\', '/');
const bare = (id: string) => slash(id.split('?')[0]!);

export interface Sorted {
  member: Set<string>;
  staff: Set<string>;
}

export function staffChunks(entries: readonly string[] = STAFF_ENTRIES) {
  let root = '';
  let sorted: Sorted | null = null;
  const isStaffEntry = (id: string) => {
    const f = bare(id);
    return entries.some((e) => f === `${root}/${e}` || f.endsWith(`/${e}`));
  };
  const isStaffChunk = (moduleIds: readonly string[]) =>
    sorted !== null && moduleIds.length > 0 && moduleIds.every((m) => sorted!.staff.has(m));

  const plugin: Plugin = {
    name: 'sideroom-staff-chunks',
    apply: 'build',
    configResolved(cfg) {
      root = slash(cfg.root);
    },
    buildEnd(error) {
      if (error) return;
      const ids = [...this.getModuleIds()];
      const roots = ids.filter((id) => this.getModuleInfo(id)?.isEntry);
      let result: Sorted;
      try {
        result = sortModules(ids, roots, (id) => this.getModuleInfo(id), isStaffEntry);
      } catch (e) {
        return this.error((e as Error).message);
      }
      sorted = result;
      const missing = entries.filter(
        (e) => ![...result.staff].some((id) => bare(id).endsWith(`/${e}`)),
      );
      if (missing.length) {
        this.error(
          `staff entries ${missing.join(', ')} are not in the build as staff-only modules: the list in vite/staffChunks.ts is stale`,
        );
      }
      // For the key audit's cross-check (scripts/staff-keys.mjs --modules): the build's own staff module list.
      const dump = process.env.SR_STAFF_MODULES_OUT;
      if (dump)
        writeFileSync(
          dump,
          JSON.stringify([...result.staff].map((id) => bare(id).replace(`${root}/`, '')).sort()),
        );
    },
    generateBundle(_o, bundle) {
      if (!sorted) return;
      const staff: string[] = [];
      for (const [name, item] of Object.entries(bundle)) {
        if (item.type === 'chunk') {
          const inStaff = item.moduleIds.filter((m) => sorted!.staff.has(m));
          if (inStaff.length && !isStaffChunk(item.moduleIds)) {
            this.error(
              `chunk ${name} mixes staff-only modules (${inStaff
                .slice(0, 3)
                .map((m) => bare(m).replace(`${root}/`, ''))
                .join(', ')}) with member modules`,
            );
          }
        }
        if (name.startsWith('staff/')) staff.push(name);
      }
      const under = (id: string) => bare(id).replace(`${root}/`, '');
      this.emitFile({
        type: 'asset',
        fileName: 'staff-chunks.json',
        source: `${JSON.stringify(
          {
            version: 1,
            entries,
            staff: staff.sort(),
            staffModules: sorted.staff.size,
            memberModulesUnderAdmin: [...sorted.member]
              .map(under)
              .filter((p) => p.startsWith('src/admin/'))
              .sort(),
            // Staff-looking code that member chunks still carry (wave 66's list); a report, not a rule.
            memberModulesStaffLooking: [...sorted.member]
              .map(under)
              .filter(
                (p) =>
                  p.startsWith('src/') &&
                  !p.startsWith('src/admin/') &&
                  !/.(test|spec)./.test(p) &&
                  !p.startsWith('src/i18n/') &&
                  /staff|platformStaff/i.test(p),
              )
              .sort(),
          },
          null,
          2,
        )}\n`,
      });
    },
  };

  /** `build.rollupOptions.output.chunkFileNames`: no `[name]` for a staff chunk, so no name tells a member anything. */
  const chunkFileNames = (chunk: { moduleIds: readonly string[] }) =>
    isStaffChunk(chunk.moduleIds) ? 'staff/[hash].js' : '[name]-[hash].js';

  /** CSS Vite emits for a staff chunk goes under `staff/` too when its source files are all staff-only. */
  const assetFileNames = (asset: { originalFileNames?: readonly string[]; names?: string[] }) => {
    const from = asset.originalFileNames ?? [];
    if (from.length && sorted && from.every((f) => sorted!.staff.has(slash(`${root}/${f}`))))
      return 'staff/[hash][extname]';
    return '[name]-[hash][extname]';
  };

  return { plugin, chunkFileNames, assetFileNames };
}
