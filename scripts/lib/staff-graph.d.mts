export const STAFF_ENTRIES: string[];
export interface ModuleNode {
  importedIds: readonly string[];
  dynamicallyImportedIds: readonly string[];
}
export function sortModules(
  all: Iterable<string>,
  entries: Iterable<string>,
  info: (id: string) => ModuleNode | null,
  isStaffEntry: (id: string) => boolean,
): { member: Set<string>; staff: Set<string> };
