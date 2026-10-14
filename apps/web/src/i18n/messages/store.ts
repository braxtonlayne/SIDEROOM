import { mergeFragments } from './merge';

/**
 * The live English catalog. Fragments register into it: the small first-paint ones (entrance, a11y, pwa)
 * from `./en.ts`, everything else from `./en.rest.ts`, which the app loads as one separate chunk so the
 * main bundle stays within its budget. Tests register both in `test/setup.ts`.
 */
export const en: Record<string, string> = {};
const owner = new Map<string, string>();

export function registerFragments(
  fragments: Readonly<Record<string, Readonly<Record<string, string>>>>,
): void {
  // Duplicates inside this batch throw here; duplicates against what is already registered throw below.
  const merged = mergeFragments(fragments);
  for (const file of Object.keys(fragments).sort()) {
    for (const key of Object.keys(fragments[file]!)) {
      const first = owner.get(key);
      if (first !== undefined && first !== file) {
        throw new Error(`Duplicate i18n key "${key}" defined in both ${first} and ${file}`);
      }
      owner.set(key, file);
      en[key] = merged[key]!;
    }
  }
}
