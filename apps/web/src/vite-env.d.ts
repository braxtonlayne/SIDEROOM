/// <reference types="vite/client" />
/** The build's version: the newest heading of CHANGELOG.md, without its date (vite.config.ts). */
declare const __SR_VERSION__: string;
/** The commit the build was made from (vite.config.ts), lowercase hex, or empty outside a checkout (OPS-CRASH-001). */
declare const __SR_COMMIT__: string;
/** The hosts hls.js may be attached to (D-518): `SR_PEERTUBE_HOSTS` and `SR_MEDIA_HOSTS` at build time (vite.config.ts). */
declare const __SR_MEDIA_HOSTS__: string[];
declare module '*.module.css' {
  const classes: Record<string, string>;
  export default classes;
}
/** The en-XA pseudo-locale catalog (vite/pseudoLocale.ts); `null` unless the dev server or SR_PSEUDO_LOCALE=1. */
declare module 'virtual:sr-pseudo-locale' {
  const catalog: Record<string, string> | null;
  export default catalog;
}
interface ImportMetaEnv {
  /** `'true'` in a build made with SR_PSEUDO_LOCALE=1 (vite.config.ts). */
  readonly VITE_SR_PSEUDO_LOCALE?: string;
}
