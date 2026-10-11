/**
 * DS-TOK-005: self-hosted WOFF2 font configuration. Files live under `/fonts/` on the
 * app's static origin (apps own the actual binaries and licence files). All faces use
 * `font-display: swap`. No third-party font CDN is ever contacted.
 */
export interface FontFaceConfig {
  family: string;
  /** Path relative to the font base URL. */
  file: string;
  weight: string;
  style: "normal" | "italic";
  /** Preload candidates are the above-the-fold faces. */
  preload?: boolean;
}

export const FONT_FACES: readonly FontFaceConfig[] = [
  { family: "Newsreader", file: "newsreader-variable.woff2", weight: "200 800", style: "normal", preload: true },
  { family: "Newsreader", file: "newsreader-italic-variable.woff2", weight: "200 800", style: "italic", preload: true },
  { family: "Hanken Grotesk", file: "hanken-grotesk-variable.woff2", weight: "100 900", style: "normal", preload: true },
  { family: "IBM Plex Mono", file: "ibm-plex-mono-400.woff2", weight: "400", style: "normal" },
  { family: "IBM Plex Mono", file: "ibm-plex-mono-500.woff2", weight: "500", style: "normal" },
  // Lounge sans: Inter is the open-licensed stand-in for Discord's proprietary face.
  { family: "Inter", file: "inter-variable.woff2", weight: "100 900", style: "normal" },
];

export function fontFaceCss(baseUrl = "/fonts/", faces: readonly FontFaceConfig[] = FONT_FACES): string {
  const base = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  return faces
    .map(
      (f) =>
        `@font-face {\n  font-family: "${f.family}";\n  src: url("${base}${f.file}") format("woff2");\n  font-weight: ${f.weight};\n  font-style: ${f.style};\n  font-display: swap;\n}`,
    )
    .join("\n\n");
}

/** `<link rel="preload">` descriptors for the head of the document. */
export function fontPreloadLinks(baseUrl = "/fonts/", faces: readonly FontFaceConfig[] = FONT_FACES) {
  const base = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  return faces
    .filter((f) => f.preload)
    .map((f) => ({ rel: "preload", as: "font", type: "font/woff2", href: `${base}${f.file}`, crossOrigin: "anonymous" }));
}
