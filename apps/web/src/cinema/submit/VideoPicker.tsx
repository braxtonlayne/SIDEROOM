// Pick video files or a whole folder, see how the names were read (the naming tester, spec
// ingest-naming-conventions) and send them to the upload manager (RDY-MED-002). A series file is
// placed by season and episode (read from the name, editable here); a film takes one file.
import { Button, Checkbox, TextInput } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { ConvertStep } from '../../convert/ConvertStep';
import { needsConversion } from '../../convert/plan';
import { useT } from '../../i18n';
import * as rv from '../../review/api';
import { fromFileList, readDropped, type PickedFile } from '../../review/folderDrop';
import type { UploadManager } from '../../review/uploadManager';
import { encodeTarget } from './uploads';

export const VIDEO_EXT = /\.(mkv|mp4|m4v|mov|avi|webm|ts|mpeg|mpg|wmv|m2ts|ogv|flv)$/i;

export interface Proposal {
  pick: PickedFile;
  /** Series only. */
  season: number | null;
  episode: number | null;
  title: string | null;
  warnings: string[];
  excluded?: boolean;
}

/** Read the names of the picked files (videos only) into a proposal. */
export async function propose(files: PickedFile[], film: boolean): Promise<Proposal[]> {
  const picks = files.filter((f) => VIDEO_EXT.test(f.path));
  if (picks.length === 0) return [];
  const parsed = await rv.parseNames(
    'cinema',
    picks.map((p) => p.path),
  );
  const rows = picks.map<Proposal>((pick, i) => {
    const c = parsed.items[i]?.cinema;
    const episode = c?.episodes[0] ?? null;
    return {
      pick,
      season: film ? null : (c?.season ?? (episode !== null ? 1 : null)),
      episode: film ? null : episode,
      title: c?.episode_title ?? null,
      warnings: c?.warnings ?? [],
    };
  });
  // A film has one video: the first stays, the rest are left out until the person chooses.
  return film ? rows.map((r, i) => (i === 0 ? r : { ...r, excluded: true })) : rows;
}

export function VideoPicker({
  submissionId,
  film,
  taken,
  manager,
  onQueued,
}: {
  submissionId: string;
  film: boolean;
  /** `season:episode` of the episodes that already have a video (they are replaced). */
  taken: Set<string>;
  manager: UploadManager;
  onQueued: () => void;
}) {
  const t = useT();
  const files = useRef<HTMLInputElement>(null);
  const folder = useRef<HTMLInputElement>(null);
  const [rows, setRows] = useState<Proposal[] | null>(null);
  const [busy, setBusy] = useState(false);
  // MKV, AVI ... are repackaged in the browser first (BYOS-CNV-001), when the person agrees.
  const [converting, setConverting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const take = async (picked: PickedFile[]) => {
    setError(null);
    try {
      setRows(await propose(picked, film));
    } catch {
      setError(t('cinema.submit.picker.readError'));
    }
  };
  const patch = (path: string, p: Partial<Proposal>) =>
    setRows((cur) => cur && cur.map((r) => (r.pick.path === path ? { ...r, ...p } : r)));
  const placed = (r: Proposal) => film || (r.season !== null && r.episode !== null);
  const ready = rows?.filter((r) => !r.excluded && placed(r)) ?? [];
  const seen = new Map<string, number>();
  for (const r of ready)
    if (!film)
      seen.set(`${r.season}:${r.episode}`, (seen.get(`${r.season}:${r.episode}`) ?? 0) + 1);
  const clashes = [...seen].filter(([, n]) => n > 1).map(([k]) => k);

  const convertible = ready.filter((r) => needsConversion(r.pick.path));
  const start = async (swap?: Map<File, File>) => {
    setBusy(true);
    setError(null);
    try {
      for (const r of ready) {
        const file = swap?.get(r.pick.file as File) ?? (r.pick.file as File);
        const target = film
          ? encodeTarget({ kind: 'auto' })
          : encodeTarget({ kind: 'numbers', season: r.season!, episode: r.episode! });
        void manager.enqueue(submissionId, target, file).catch(() => undefined);
      }
      setRows(null);
      onQueued();
    } catch {
      setError(t('cinema.submit.picker.startError'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="rv-stack"
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        void readDropped(e.dataTransfer.items).then(take);
      }}
    >
      <p className="rv-muted">
        {film ? t('cinema.submit.picker.dropFilm') : t('cinema.submit.picker.dropSeries')}
      </p>
      <div className="rv-row">
        <Button variant="secondary" onClick={() => files.current?.click()}>
          {t('cinema.submit.picker.chooseFiles')}
        </Button>
        <Button variant="secondary" onClick={() => folder.current?.click()}>
          {t('cinema.submit.picker.chooseFolder')}
        </Button>
      </div>
      <input
        ref={files}
        type="file"
        multiple={!film}
        hidden
        accept="video/*,.mkv,.m2ts,.ts"
        aria-label={t('cinema.submit.picker.filesInput')}
        onChange={(e) => {
          if (e.target.files) void take(fromFileList(e.target.files));
          if (files.current) files.current.value = '';
        }}
      />
      <input
        ref={folder}
        type="file"
        multiple
        hidden
        // Not in the React types: lets the person pick a whole folder.
        {...({ webkitdirectory: '' } as Record<string, string>)}
        aria-label={t('cinema.submit.picker.folderInput')}
        onChange={(e) => {
          if (e.target.files) void take(fromFileList(e.target.files));
          if (folder.current) folder.current.value = '';
        }}
      />
      {error && <p role="alert">{error}</p>}
      {rows && (
        <section aria-label={t('cinema.submit.picker.proposal')} className="rv-stack">
          <h3>{t('cinema.submit.picker.proposal')}</h3>
          {rows.length === 0 ? (
            <p className="rv-muted">{t('cinema.submit.picker.nothing')}</p>
          ) : (
            <ul className="rv-changes">
              {rows.map((r) => (
                <li key={r.pick.path}>
                  <Checkbox
                    label={t('cinema.submit.picker.include', { path: r.pick.path })}
                    checked={!r.excluded}
                    onChange={(e) => patch(r.pick.path, { excluded: !e.target.checked })}
                  />
                  {!film && (
                    <span className="rv-row">
                      <TextInput
                        label={t('cinema.submit.picker.season', { path: r.pick.path })}
                        inputMode="numeric"
                        value={r.season === null ? '' : String(r.season)}
                        disabled={r.excluded === true}
                        onChange={(e) => patch(r.pick.path, { season: whole(e.target.value) })}
                      />
                      <TextInput
                        label={t('cinema.submit.picker.episode', { path: r.pick.path })}
                        inputMode="numeric"
                        value={r.episode === null ? '' : String(r.episode)}
                        disabled={r.excluded === true}
                        onChange={(e) => patch(r.pick.path, { episode: whole(e.target.value) })}
                      />
                    </span>
                  )}
                  {!r.excluded && !placed(r) && (
                    <span role="status">{t('cinema.submit.picker.unplaced')}</span>
                  )}
                  {!r.excluded && !film && placed(r) && taken.has(`${r.season}:${r.episode}`) && (
                    <span role="status">
                      {t('cinema.submit.picker.replaces', {
                        season: r.season!,
                        episode: r.episode!,
                      })}
                    </span>
                  )}
                  {r.excluded && film && <span>{t('cinema.submit.picker.oneFilm')}</span>}
                  {r.warnings.map((w) => (
                    <span key={w} className="rv-muted">
                      {w}
                    </span>
                  ))}
                </li>
              ))}
            </ul>
          )}
          {clashes.map((k) => (
            <p key={k} role="alert">
              {t('cinema.submit.picker.duplicate', {
                season: k.split(':')[0] ?? '',
                episode: k.split(':')[1] ?? '',
              })}
            </p>
          ))}
          {converting && (
            <ConvertStep
              files={convertible.map((r) => r.pick.file as File)}
              subtitles={false}
              onDone={(items) => {
                setConverting(false);
                void start(new Map(items.map((i) => [i.original, i.output.file])));
              }}
              onAsIs={() => {
                setConverting(false);
                void start();
              }}
              onCancel={() => setConverting(false)}
            />
          )}
          <div className="rv-row">
            <Button
              onClick={() => (convertible.length > 0 ? setConverting(true) : void start())}
              loading={busy}
              disabled={ready.length === 0 || clashes.length > 0 || converting}
            >
              {t('cinema.submit.picker.upload', { count: ready.length })}
            </Button>
            <Button variant="secondary" onClick={() => setRows(null)}>
              {t('cinema.submit.picker.discard')}
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}

/** A whole number from what was typed, or null. */
function whole(v: string): number | null {
  const s = v.trim();
  if (!/^\d{1,4}$/.test(s)) return null;
  return Number(s);
}
