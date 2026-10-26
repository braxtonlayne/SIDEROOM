import { formatNodes, parseIcu, type FormatOptions, type Node, type Values } from './icu';
import { en, type MessageKey } from './messages/en';
import { isRestLoaded, onRestLoaded } from './messages/store';
import { wrapPseudo } from './pseudo';

/**
 * Locale registry and translate function (XC-I18N-001/002).
 *
 * - `en` is the source catalog and is always bundled; it is also the fallback for any missing key.
 * - `en-XA` (accented, +35% length, bracketed) and `ar-XB` (the same text, right-to-left) are pseudo
 *   locales derived from `en` at runtime, for tests and for eyeballing layout.
 * - Real translations register a lazy loader in `LOADERS`, so only the active catalog is fetched.
 * - `t()` works without any React provider; the React hooks in `./react` subscribe to locale changes.
 */

export type Catalog = Readonly<Partial<Record<MessageKey, string>>>;

export interface LocaleInfo {
  code: string;
  /** Endonym, shown in the picker. */
  label: string;
  dir: 'ltr' | 'rtl';
  pseudo?: boolean;
  /** The catalog is generated at build time and already carries the pseudo text, so `t()` adds nothing. */
  generated?: boolean;
}

/**
 * RDY-I18N-002: the pseudo locales exist on development hosts and in a build made with SR_PSEUDO_LOCALE=1
 * (served by `vite preview`). `import.meta.env.DEV` is a literal `false` in a production build and the
 * second flag is replaced by `vite.config.ts`, so the whole branch, `pseudo.ts` and the generated catalog
 * included, is dropped from the bundle (scripts/check-bundle-budget.mjs asserts it).
 */
export const PSEUDO_ENABLED: boolean =
  import.meta.env.DEV || import.meta.env.VITE_SR_PSEUDO_LOCALE === 'true';

export const LOCALES: readonly LocaleInfo[] = [
  { code: 'en', label: 'English', dir: 'ltr' },
  ...(PSEUDO_ENABLED
    ? ([
        { code: 'en-XA', label: 'Pseudo (accented)', dir: 'ltr', pseudo: true, generated: true },
        { code: 'ar-XB', label: 'Pseudo (right-to-left)', dir: 'rtl', pseudo: true },
      ] satisfies LocaleInfo[])
    : []),
];

/** Lazy catalog loaders for real translations: `'de': () => import('./messages/de').then((m) => m.de)`. */
const LOADERS: Record<string, () => Promise<Catalog>> = {};
// en-XA is generated from the English fragments by scripts/pseudo-locale.mjs (vite/pseudoLocale.ts).
if (PSEUDO_ENABLED) {
  LOADERS['en-XA'] = () =>
    import('virtual:sr-pseudo-locale').then((m) => (m.default ?? {}) as Catalog);
}

const STORAGE_KEY = 'sr.locale';
const HOUR_KEY = 'sr.hour12';

type Listener = () => void;
const listeners = new Set<Listener>();
let current = 'en';
/** True while a `?lang=` pseudo preview is showing (nothing saved); a device-level default must not replace it. */
let preview = false;
export const isPreviewLocale = () => preview;
let hour12: boolean | undefined;
let loaded: Catalog | undefined;
let version = 0;
const compiled = new Map<string, Node[]>();

function emit() {
  version++;
  for (const l of listeners) l();
}

export function subscribe(l: Listener): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Changes whenever the locale or clock preference changes (for `useSyncExternalStore`). */
export const getVersion = () => version;
export const getLocale = () => current;
export const getHour12 = () => hour12;

export function localeInfo(code: string = current): LocaleInfo {
  return LOCALES.find((l) => l.code === code) ?? LOCALES[0]!;
}

/** The best supported locale for a list of BCP 47 tags (such as `navigator.languages`). */
export function negotiateLocale(wanted: readonly string[]): string {
  const real = LOCALES.filter((l) => !l.pseudo).map((l) => l.code);
  for (const w of wanted) {
    const exact = real.find((c) => c.toLowerCase() === w.toLowerCase());
    if (exact) return exact;
    const base = real.find((c) => c.toLowerCase() === w.split('-')[0]!.toLowerCase());
    if (base) return base;
  }
  return 'en';
}

function applyToDocument(info: LocaleInfo) {
  if (typeof document === 'undefined') return;
  document.documentElement.lang =
    info.code === 'ar-XB' ? 'ar' : info.code === 'en-XA' ? 'en' : info.code;
  document.documentElement.dir = info.dir;
}

/** Switch locale: loads the catalog when needed, persists the choice, updates `<html lang dir>`. */
export async function setLocale(code: string, persist = true): Promise<void> {
  preview = !persist;
  const info = LOCALES.find((l) => l.code === code) ?? LOCALES[0]!;
  const loader = LOADERS[info.code];
  loaded = loader ? await loader() : undefined;
  current = info.code;
  compiled.clear();
  if (persist) {
    try {
      localStorage.setItem(STORAGE_KEY, info.code);
    } catch {
      /* storage can be unavailable; the choice then lasts for this tab only */
    }
  }
  applyToDocument(info);
  emit();
}

/** 12 or 24 hour clock for `{t, time}` arguments; `undefined` follows the locale. */
export function setHour12(value: boolean | undefined) {
  hour12 = value;
  try {
    if (value === undefined) localStorage.removeItem(HOUR_KEY);
    else localStorage.setItem(HOUR_KEY, String(value));
  } catch {
    /* ignore */
  }
  emit();
}

/** Restore the saved locale (or negotiate from the browser). Call once at startup. */
let fullCatalog: Promise<unknown> | undefined;

/** Loads the rest of the English catalog (everything but the first-paint fragments) as its own chunk. */
export function loadFullCatalog(): Promise<unknown> {
  return (fullCatalog ??= import('./messages/en.rest'));
}

// The catalog grew: re-render every `useT` subscriber so text that used the fallback is refreshed.
onRestLoaded(emit);

let failed = false;
/** True once the rest catalog is registered, or its load failed (the app then opens degraded, not blank). */
export const fullCatalogSettled = () => isRestLoaded() || failed;

/**
 * Start loading the rest catalog without blocking anything. If the chunk fails (offline, stale deploy) the
 * gates open anyway; affected strings then show their keys.
 */
export function startFullCatalog(): void {
  loadFullCatalog().catch((e: unknown) => {
    console.error('Could not load the full message catalog', e);
    failed = true;
    emit();
  });
}

export async function initLocale(): Promise<void> {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
    const h = localStorage.getItem(HOUR_KEY);
    hour12 = h === null ? undefined : h === 'true';
  } catch {
    /* ignore */
  }
  const fromBrowser = negotiateLocale(
    typeof navigator === 'undefined' ? [] : (navigator.languages ?? []),
  );
  // The rest catalog loads in parallel and is NOT awaited: first paint only needs the core fragments.
  startFullCatalog();
  // `?lang=en-XA` (or `ar-XB`) previews a pseudo locale for this page load only; it is never saved.
  const wanted = PSEUDO_ENABLED ? pseudoFromQuery() : undefined;
  if (wanted) return setLocale(wanted, false);
  await setLocale(saved && LOCALES.some((l) => l.code === saved) ? saved : fromBrowser);
}

function pseudoFromQuery(): string | undefined {
  try {
    const q = new URLSearchParams(location.search).get('lang');
    return LOCALES.find((l) => l.pseudo && l.code === q)?.code;
  } catch {
    return undefined;
  }
}

function messageFor(key: MessageKey): string {
  return loaded?.[key] ?? en[key] ?? key;
}

function compile(key: MessageKey): Node[] {
  const cacheKey = `${current}\u0000${key}`;
  let nodes = compiled.get(cacheKey);
  if (!nodes) {
    nodes = parseIcu(messageFor(key));
    const info = localeInfo();
    if (PSEUDO_ENABLED && info.pseudo && !info.generated) nodes = wrapPseudo(nodes);
    compiled.set(cacheKey, nodes);
  }
  return nodes;
}

const warned = new Set<string>();

/** Translate a catalog key. Unknown keys return the key itself so they stand out. */
export function t(key: MessageKey, values: Values = {}): string {
  if (!(key in en)) {
    if (import.meta.env.DEV && !isRestLoaded() && !warned.has(key)) {
      warned.add(key);
      console.warn(
        `i18n: "${key}" used before the full catalog loaded. Routes outside the entrance must render behind CatalogGate; entrance keys belong in the entrance/a11y/pwa fragments.`,
      );
    }
    return key;
  }
  const info = localeInfo();
  const intl = info.code === 'en-XA' ? 'en' : info.code === 'ar-XB' ? 'ar' : info.code;
  const opts: FormatOptions = hour12 === undefined ? {} : { hour12 };
  try {
    return formatNodes(compile(key), values, intl, opts);
  } catch {
    return en[key] ?? key;
  }
}

/** Locale-aware helpers for UI code that formats outside a message. */
export function formatNumber(n: number, o?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(intlTag(), o).format(n);
}

/** Dates and times never throw: an unparseable value renders like `Date#toLocaleString` did. */
function safeFormat(d: Date | number, make: () => Intl.DateTimeFormat): string {
  try {
    return make().format(d);
  } catch {
    return 'Invalid Date';
  }
}

/** A date. With empty options this is the numeric short form (`1/31/2026`), like `toLocaleDateString()`. */
export function formatDate(
  d: Date | number,
  o: Intl.DateTimeFormatOptions = { dateStyle: 'medium' },
): string {
  return safeFormat(d, () => new Intl.DateTimeFormat(intlTag(), o));
}

/** A clock time; honours the 12/24 hour preference. */
export function formatTime(
  d: Date | number,
  o: Intl.DateTimeFormatOptions = { timeStyle: 'short' },
): string {
  return safeFormat(d, () => new Intl.DateTimeFormat(intlTag(), withHour12(o)));
}

/** Date and time together (default: numeric date with seconds, like `toLocaleString()`); honours 12/24 hour. */
export function formatDateTime(
  d: Date | number,
  o: Intl.DateTimeFormatOptions = {
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
  },
): string {
  return safeFormat(d, () => new Intl.DateTimeFormat(intlTag(), withHour12(o)));
}

export function formatRelative(value: number, unit: Intl.RelativeTimeFormatUnit): string {
  return new Intl.RelativeTimeFormat(intlTag(), { numeric: 'auto' }).format(value, unit);
}

/** `hour12` only applies when the caller did not choose a clock. */
function withHour12(o: Intl.DateTimeFormatOptions): Intl.DateTimeFormatOptions {
  if (hour12 === undefined || o.hour12 !== undefined || o.hourCycle !== undefined) return o;
  return { ...o, hour12 };
}

/**
 * Region override for dates and numbers (User Settings > Language "Language and region"). `undefined`
 * follows the UI language.
 */
let formatLocale: string | undefined;
export function setFormatLocale(code: string | undefined) {
  let next = code;
  try {
    if (next) next = Intl.getCanonicalLocales(next)[0];
  } catch {
    next = undefined;
  }
  if (formatLocale === next) return;
  formatLocale = next;
  emit();
}
export const getFormatLocale = () => formatLocale;

function intlTag(): string {
  if (formatLocale) return formatLocale;
  return current === 'en-XA' ? 'en' : current === 'ar-XB' ? 'ar' : current;
}

/** Register a lazily loaded translation (used by catalogs added later and by tests). */
export function registerLocale(info: LocaleInfo, loader: () => Promise<Catalog>) {
  if (!LOCALES.some((l) => l.code === info.code)) (LOCALES as LocaleInfo[]).push(info);
  LOADERS[info.code] = loader;
}

/** Remove a registered locale (tests). */
export function unregisterLocale(code: string) {
  const i = LOCALES.findIndex((l) => l.code === code);
  if (i >= 0) (LOCALES as LocaleInfo[]).splice(i, 1);
  delete LOADERS[code];
}
