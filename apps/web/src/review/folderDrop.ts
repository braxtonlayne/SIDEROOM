// Folder drag and drop and the directory picker (REV-UP-003): turn a dropped folder (DataTransferItem
// entries) or a `webkitdirectory` pick into a flat list of files with their paths, so the importer can
// read the structure from the names (the naming parser proposes it, the person adjusts it).

export interface PickedFile {
  file: File;
  /** Path from the dropped folder's parent, `/` separated, ending in the file name. */
  path: string;
}

/** Most files taken from one drop, deepest folder level followed. */
export const MAX_FILES = 5000;
export const MAX_DEPTH = 8;

interface Entry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  file?: (ok: (f: File) => void, err: (e: unknown) => void) => void;
  createReader?: () => {
    readEntries: (ok: (e: Entry[]) => void, err: (e: unknown) => void) => void;
  };
}

const readAll = async (dir: Entry): Promise<Entry[]> => {
  const reader = dir.createReader!();
  const out: Entry[] = [];
  // readEntries returns at most ~100 at a time: ask until it answers empty.
  for (;;) {
    const batch = await new Promise<Entry[]>((ok, err) => reader.readEntries(ok, err));
    if (batch.length === 0) return out;
    out.push(...batch);
    if (out.length > MAX_FILES * 2) return out;
  }
};

async function walk(entry: Entry, prefix: string, depth: number, out: PickedFile[]): Promise<void> {
  if (out.length >= MAX_FILES) return;
  if (entry.isFile && entry.file) {
    const f = await new Promise<File>((ok, err) => entry.file!(ok, err));
    out.push({ file: f, path: `${prefix}${entry.name}` });
  } else if (entry.isDirectory && depth < MAX_DEPTH) {
    for (const child of await readAll(entry))
      await walk(child, `${prefix}${entry.name}/`, depth + 1, out);
  }
}

/** Everything under a drop: folders are walked, loose files are kept as they are. */
export async function readDropped(
  items: DataTransferItemList | DataTransferItem[],
): Promise<PickedFile[]> {
  const out: PickedFile[] = [];
  // Entries must be taken synchronously: the list is cleared once the event handler returns.
  const entries: Entry[] = [];
  const loose: File[] = [];
  for (const item of Array.from(items)) {
    if (item.kind !== 'file') continue;
    const e = (
      item as DataTransferItem & { webkitGetAsEntry?: () => Entry | null }
    ).webkitGetAsEntry?.();
    if (e) entries.push(e);
    else {
      const f = item.getAsFile();
      if (f) loose.push(f);
    }
  }
  for (const e of entries) await walk(e, '', 0, out);
  for (const f of loose) out.push({ file: f, path: f.name });
  return out;
}

/** The files of a `<input type="file" webkitdirectory>` pick. */
export function fromFileList(files: FileList | File[]): PickedFile[] {
  return Array.from(files)
    .slice(0, MAX_FILES)
    .map((file) => ({
      file,
      path: (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name,
    }));
}

/** The files by their first folder (one series folder per group); loose files share the group "". */
export function groupByFolder(files: PickedFile[]): Map<string, PickedFile[]> {
  const out = new Map<string, PickedFile[]>();
  for (const f of files) {
    const i = f.path.indexOf('/');
    const key = i < 0 ? '' : f.path.slice(0, i);
    out.set(key, [...(out.get(key) ?? []), f]);
  }
  return out;
}

const ARCHIVE = /\.(cbz|zip|cb7|7z|cbr|rar|epub|pdf)$/i;

/** Only files an upload can take; everything else in the folder (covers, notes) is left out. */
export const uploadable = (files: PickedFile[]): PickedFile[] =>
  files.filter((f) => ARCHIVE.test(f.path));
