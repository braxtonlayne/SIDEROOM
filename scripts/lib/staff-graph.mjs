// The staff module-graph rule (D-651, D-663), shared by the build plugin (apps/web/vite/staffChunks.ts) and the key
// audit (scripts/staff-keys.mjs), so that "which module is staff-only" has ONE definition.
//
//  - the MEMBER SET is every module reached from the app entries through static and dynamic imports, without
//    following a dynamic import whose target is a staff entry;
//  - every other module of the build is STAFF-ONLY.

/** Staff entries, relative to apps/web, forward slashes. Member code reaches them only through a dynamic import(). */
export const STAFF_ENTRIES = ['src/admin/AdminApp.tsx', 'src/i18n/messages/en.staff.ts'];

const slash = (p) => p.replaceAll('\\', '/');
const bare = (id) => slash(id.split('?')[0]);

/**
 * Pure graph walk. `all` is every module id of the build, `entries` the app entry modules,
 * `info(id)` gives `{ importedIds, dynamicallyImportedIds }` or null.
 * @returns {{ member: Set<string>, staff: Set<string> }}
 */
export function sortModules(all, entries, info, isStaffEntry) {
  const member = new Set();
  const stack = [...entries];
  while (stack.length) {
    const id = stack.pop();
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
  const staff = new Set();
  for (const id of all) if (!member.has(id)) staff.add(id);
  return { member, staff };
}
