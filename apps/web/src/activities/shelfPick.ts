// D-466: what the shelf picker lists for an Activity's `pickShelfObject { accept, max_size }`. Pure rules, shared by the
// picker dialog (which filters what the member sees) and the host bridge (which checks the pick again before it hands
// an Activity anything, so a buggy or swapped picker cannot widen the filter).

/** Media types the picker can name from a file's extension; anything else is `application/octet-stream`. */
const TYPES: Record<string, string> = {
  json: 'application/json',
  txt: 'text/plain',
  text: 'text/plain',
  md: 'text/markdown',
  csv: 'text/csv',
  tsv: 'text/tab-separated-values',
  xml: 'application/xml',
  html: 'text/html',
  css: 'text/css',
  js: 'text/javascript',
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  avif: 'image/avif',
  svg: 'image/svg+xml',
  mp3: 'audio/mpeg',
  ogg: 'audio/ogg',
  opus: 'audio/ogg',
  wav: 'audio/wav',
  flac: 'audio/flac',
  m4a: 'audio/mp4',
  mp4: 'video/mp4',
  webm: 'video/webm',
  mkv: 'video/x-matroska',
  pdf: 'application/pdf',
  epub: 'application/epub+zip',
  zip: 'application/zip',
};

/** The last part of a shelf path. */
export const fileName = (path: string): string => path.split('/').filter(Boolean).pop() ?? path;

/** Lower-case extension without the dot; '' when there is none. */
function ext(name: string): string {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

/** A media type guessed from the extension of `name`. */
export function contentTypeOf(name: string): string {
  return TYPES[ext(name)] ?? 'application/octet-stream';
}

/**
 * Whether a file passes the Activity's `accept` list: ".json" or "json" matches the extension, "text/plain" the media
 * type exactly and "image/*" a whole family. An empty or absent list passes everything.
 */
export function acceptMatches(name: string, accept: readonly string[] | undefined): boolean {
  if (!accept || accept.length === 0) return true;
  const e = ext(name);
  const type = contentTypeOf(name);
  return accept.some((raw) => {
    const a = raw.trim().toLowerCase();
    if (!a) return false;
    if (a.includes('/')) {
      if (a.endsWith('/*')) return type.startsWith(a.slice(0, -1));
      return type === a;
    }
    return e !== '' && e === a.replace(/^\./, '');
  });
}

/** Whether a file of `size` bytes called `name` is listed for these filters. */
export function pickable(
  name: string,
  size: number,
  req: { accept?: readonly string[] | undefined; maxSize?: number | null | undefined },
): boolean {
  if (req.maxSize != null && size > req.maxSize) return false;
  return acceptMatches(name, req.accept);
}
