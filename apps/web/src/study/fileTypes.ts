// The Study's type filters (STU-SHF-001): decided from the key alone, by folder and extension, never by asking an
// outside service. `emu/` is the emulator's folder (ROMs, saves, BIOS), so everything under it is a game file.

export type StudyType = 'video' | 'audio' | 'reading' | 'games' | 'images' | 'other';
export const STUDY_TYPES: readonly StudyType[] = [
  'video',
  'audio',
  'reading',
  'games',
  'images',
  'other',
];

const EXT: Record<Exclude<StudyType, 'other'>, readonly string[]> = {
  video: [
    'mp4',
    'm4v',
    'webm',
    'mkv',
    'avi',
    'mov',
    'ts',
    'm2ts',
    'mpg',
    'mpeg',
    'ogv',
    'wmv',
    'flv',
  ],
  audio: ['mp3', 'flac', 'ogg', 'oga', 'opus', 'm4a', 'aac', 'wav', 'wma', 'aiff', 'alac', 'mka'],
  reading: ['epub', 'cbz', 'cbr', 'cb7', 'pdf', 'mobi', 'azw3', 'fb2', 'djvu'],
  // Cartridges, disc images and the mod containers games load (a plain .zip stays "other").
  games: [
    'nes',
    'sfc',
    'smc',
    'gb',
    'gbc',
    'gba',
    'md',
    'gen',
    'smd',
    'nds',
    'z64',
    'n64',
    'cue',
    'iso',
    'chd',
    'pbp',
    'wad',
    'pk3',
    'pk4',
    'pak',
    'vpk',
    'bsp',
    'mrpack',
    'mcpack',
    'esp',
    'esm',
    'bsa',
  ],
  images: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'avif', 'bmp', 'svg', 'heic', 'tif', 'tiff'],
};

const BY_EXT = new Map<string, StudyType>();
for (const [type, exts] of Object.entries(EXT))
  for (const e of exts) BY_EXT.set(e, type as StudyType);

/** The last path segment of a key (a folder key's own name). */
export const baseName = (key: string): string => key.split('/').filter(Boolean).pop() ?? key;

/** Lower-case extension without the dot; '' when there is none. */
export function extOf(key: string): string {
  const name = baseName(key);
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

/** Which filter a file belongs to. */
export function typeOf(key: string): StudyType {
  if (key.startsWith('emu/')) return 'games';
  return BY_EXT.get(extOf(key)) ?? 'other';
}

/** A file name as a title: extension and separators out. */
export function titleOf(key: string): string {
  const name = baseName(key);
  const ext = extOf(key);
  const stem = ext ? name.slice(0, -(ext.length + 1)) : name;
  return stem.replace(/[._]+/g, ' ').replace(/\s+/g, ' ').trim() || name;
}

/** Apply the type filter (`null` = all). */
export function filterByType<T extends { key: string }>(
  files: readonly T[],
  type: StudyType | null,
): T[] {
  return type === null ? [...files] : files.filter((f) => typeOf(f.key) === type);
}

/** How many files each type holds (the numbers on the filter chips). */
export function countByType(files: readonly { key: string }[]): Record<StudyType, number> {
  const n: Record<StudyType, number> = {
    video: 0,
    audio: 0,
    reading: 0,
    games: 0,
    images: 0,
    other: 0,
  };
  for (const f of files) n[typeOf(f.key)] += 1;
  return n;
}
