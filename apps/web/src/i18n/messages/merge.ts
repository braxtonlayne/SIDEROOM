/**
 * Merge catalog fragments into one object. Throws when two fragments define the same key, so a duplicate
 * is caught by any test run (and by the dev server) instead of one string silently overriding another.
 * `fragments` maps a file name (used in the error message) to that fragment's messages.
 */
export function mergeFragments(
  fragments: Readonly<Record<string, Readonly<Record<string, string>>>>,
): Record<string, string> {
  const merged: Record<string, string> = {};
  const owner = new Map<string, string>();
  for (const file of Object.keys(fragments).sort()) {
    for (const [key, value] of Object.entries(fragments[file]!)) {
      const first = owner.get(key);
      if (first !== undefined) {
        throw new Error(`Duplicate i18n key "${key}" defined in both ${first} and ${file}`);
      }
      owner.set(key, file);
      merged[key] = value;
    }
  }
  return merged;
}
