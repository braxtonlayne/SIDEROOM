/// <reference types="vite/client" />
/** The build's version: the newest heading of CHANGELOG.md, without its date (vite.config.ts). */
declare const __SR_VERSION__: string;
declare module '*.module.css' {
  const classes: Record<string, string>;
  export default classes;
}
