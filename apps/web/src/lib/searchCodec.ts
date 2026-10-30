/**
 * Search values are JSON-parsed like the router's default, except integers a JS number cannot hold exactly:
 * a snowflake such as `?submission=107586859906891776` stays a string. The default turned it into a number and
 * rounded it, so staff card links (G-68) opened a submission that does not exist.
 * Shared by the app router and every public surface's router.
 */
const isUnsafeInteger = (raw: string) => /^-?\d+$/.test(raw) && !Number.isSafeInteger(Number(raw));

export function parseSearchValue(raw: string): unknown {
  if (isUnsafeInteger(raw)) return raw;
  return JSON.parse(raw);
}

/** The inverse: such an id is written back as bare digits, not as a quoted JSON string. */
export function stringifySearchValue(value: unknown): string {
  if (typeof value === 'string' && isUnsafeInteger(value)) return value;
  return JSON.stringify(value);
}
