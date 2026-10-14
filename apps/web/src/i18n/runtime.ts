import { formatNodes, parseIcu, type FormatOptions, type Node, type Values } from './icu';
import { en, type MessageKey } from './messages/en';
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
}

export const LOCALES: readonly LocaleInfo[] = [
  { code: 'en', label: 'English', dir: 'ltr' },
  { code: 'en-XA', label: 'Pseudo (accented)', dir: 'ltr', pseudo: true },
  { code: 'ar-XB', label: 'Pseudo (right-to-left)', dir: 'rtl', pseudo: true },
];

/** Lazy catalog loaders for real translations: `'de': () => import('./messages/de').then((m) => m.de)`. */
const LOADERS: Record<string, () => Promise<Catalog>> = {};

const STORAGE_KEY = 'sr.locale';
const HOUR_KEY = 'sr.hour12';

type Listener = () => void;
const listeners = new Set<Listener>();
let current = 'en';
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
export async function setLocale(code: string): Promise<void> {
  const info = LOCALES.find((l) => l.code === code) ?? LOCALES[0]!;
  const loader = LOADERS[info.code];
  loaded = loader ? await loader() : undefined;
  current = info.code;
  compiled.clear();
  try {
    localStorage.setItem(STORAGE_KEY, info.code);
  } catch {
    /* storage can be unavailable; the choice then lasts for this tab only */
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
  await Promise.all([
    loadFullCatalog(),
    setLocale(saved && LOCALES.some((l) => l.code === saved) ? saved : fromBrowser),
  ]);
}

function messageFor(key: MessageKey): string {
  return loaded?.[key] ?? en[key] ?? key;
}

function compile(key: MessageKey): Node[] {
  const cacheKey = `${current}\u0000${key}`;
  let nodes = compiled.get(cacheKey);
  if (!nodes) {
    nodes = parseIcu(messageFor(key));
    if (localeInfo().pseudo) nodes = wrapPseudo(nodes);
    compiled.set(cacheKey, nodes);
  }
  return nodes;
}

/** Translate a catalog key. Unknown keys return the key itself so they stand out. */
export function t(key: MessageKey, values: Values = {}): string {
  if (!(key in en)) return key;
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
export function formatDate(
  d: Date | number,
  o: Intl.DateTimeFormatOptions = { dateStyle: 'medium' },
): string {
  return new Intl.DateTimeFormat(intlTag(), o).format(d);
}
export function formatTime(
  d: Date | number,
  o: Intl.DateTimeFormatOptions = { timeStyle: 'short' },
): string {
  return new Intl.DateTimeFormat(intlTag(), hour12 === undefined ? o : { ...o, hour12 }).format(d);
}
export function formatRelative(value: number, unit: Intl.RelativeTimeFormatUnit): string {
  return new Intl.RelativeTimeFormat(intlTag(), { numeric: 'auto' }).format(value, unit);
}
function intlTag(): string {
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
