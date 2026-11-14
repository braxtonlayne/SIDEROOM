import type { Plugin } from 'vite';

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
 * HOW TO ADD A STAFF ENTRY: add its path (relative to `apps/web`, forward slashes) to `STAFF_ENTRIES`, reach it from
 * member code ONLY through a dynamic `import()` made by a small door that asks the server first (see
 * `src/admin/StaffDoor.tsx`), and run the build and `node scripts/check-staff-delivery.mjs`. A static import of a staff
 * entry from member code fails the build, naming the importer.
 */
export const STAFF_ENTRIES: readonly string[] = ['src/admin/AdminApp.tsx'];

export interface ModuleNode {
  importedIds: readonly string[];
  dynamicallyImportedIds: readonly string[];
}

const slash = (p: string) => p.replaceAll('\\', '/');
const bare = (id: string) => slash(id.split('?')[0]!);

export interface Sorted {
  member: Set<string>;
  staff: Set<string>;
}

/** Pure graph walk; `all` is every module id of the build, `entries` the app entry modules. */
export function sortModules(
  all: Iterable<string>,
  entries: Iterable<string>,
  info: (id: string) => ModuleNode | null,
  isStaffEntry: (id: string) => boolean,
): Sorted {
  const member = new Set<string>();
  const stack: string[] = [...entries];
  while (stack.length) {
    const id = stack.pop()!;
    if (member.has(id)) continue;
    member.add(id);
    const node = info(id);
    if (!node) continue;
    for (const next of node.importedIds) {
      if (isStaffEntry(next)) {
        throw new Error(
          `staff entry ${bare(next)} is imported statically by ${bare(id)}: reach it only through a dynamic import() ` +
            'made by a door that asks the server first (D-651)',
        );
      }
      stack.push(next);
    }
    for (const next of node.dynamicallyImportedIds) if (!isStaffEntry(next)) stack.push(next);
  }
  const staff = new Set<string>();
  for (const id of all) if (!member.has(id)) staff.add(id);
  return { member, staff };
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
      if (![...result.staff].some(isStaffEntry)) {
        this.error(
          `staff entries ${entries.join(', ')} are not in the build as staff-only modules: the list in vite/staffChunks.ts is stale`,
        );
      }
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
