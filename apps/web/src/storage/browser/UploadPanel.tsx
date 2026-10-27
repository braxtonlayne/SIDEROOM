// The upload side of the bucket browser (BYOS-BRW-001): pick files or a folder, or drop them (a dropped folder is
// walked with review/folderDrop.ts), and a dock shows every upload of this shelf with pause, resume, cancel and
// one overall bar. The files go straight to the bucket in 16 MiB parts (mountUploads.ts); a refresh continues them.
import { Button } from '@sideroom/ui';
import { useEffect, useRef, useState, useSyncExternalStore, type DragEvent } from 'react';
import { useT } from '../../i18n';
import { fromFileList, readDropped, type PickedFile } from '../../review/folderDrop';
import { uploadKey } from './keys';
import { getMountUploads, type MountUploadManager, type MountUploadTask } from './mountUploads';
import { taskErrorText } from './writeErrors';

const pct = (n: number) => Math.round(Math.min(1, Math.max(0, n)) * 100);

export function useMountUploads(manager: MountUploadManager) {
  const tasks = useSyncExternalStore(
    (fn) => manager.subscribe(fn),
    () => manager.snapshot(),
  );
  return { tasks, total: manager.total() };
}

export function UploadPanel({
  mountId,
  prefix,
  manager,
  onUploaded,
}: {
  mountId: string;
  prefix: string;
  manager?: MountUploadManager;
  /** Called when a file has finished, so the listing can refresh. */
  onUploaded: () => void;
}) {
  const t = useT();
  const m = manager ?? getMountUploads();
  const { tasks: all, total } = useMountUploads(m);
  const tasks = all.filter((x) => x.mountId === mountId);
  const files = useRef<HTMLInputElement>(null);
  const folder = useRef<HTMLInputElement>(null);
  const [skipped, setSkipped] = useState(0);
  const [over, setOver] = useState(false);

  // Pick up what a closed page left behind.
  useEffect(() => {
    void m.resumeAll(mountId);
  }, [m, mountId]);

  // A finished file changes the listing.
  const doneCount = tasks.filter((x) => x.state === 'done').length;
  const seen = useRef(0);
  useEffect(() => {
    if (doneCount > seen.current) onUploaded();
    seen.current = doneCount;
  }, [doneCount, onUploaded]);

  const take = (picked: PickedFile[]) => {
    let bad = 0;
    for (const p of picked) {
      const key = p.file.size > 0 ? uploadKey(prefix, p.path) : null;
      if (key === null) {
        bad += 1;
        continue;
      }
      void m.enqueue(mountId, key, p.file).catch(() => undefined);
    }
    setSkipped(bad);
  };
  const onFiles = (list: FileList | null) => {
    if (list) take(fromFileList(list));
  };
  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setOver(false);
    void readDropped(e.dataTransfer.items).then(take);
  };

  const stateText = (x: MountUploadTask) =>
    t(
      x.state === 'paused'
        ? 'storage.write.dock.statePaused'
        : x.state === 'done'
          ? 'storage.write.dock.stateDone'
          : x.state === 'failed'
            ? 'storage.write.dock.stateFailed'
            : x.state === 'running'
              ? 'storage.write.dock.stateRunning'
              : 'storage.write.dock.stateQueued',
    );
  const where = prefix === '' ? t('storage.write.upload.top') : prefix;

  return (
    <section className="sto-upload">
      <div
        className={`sto-drop${over ? ' sto-drop-over' : ''}`}
        role="group"
        aria-label={t('storage.write.upload.zone')}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={onDrop}
      >
        <p className="us-muted">{t('storage.write.upload.drop', { path: where })}</p>
        <div className="us-actions">
          <Button variant="secondary" onClick={() => files.current?.click()}>
            {t('storage.write.upload.files')}
          </Button>
          <Button variant="secondary" onClick={() => folder.current?.click()}>
            {t('storage.write.upload.folder')}
          </Button>
        </div>
        <input
          ref={files}
          type="file"
          multiple
          hidden
          data-testid="sto-pick-files"
          onChange={(e) => {
            onFiles(e.target.files);
            e.target.value = '';
          }}
        />
        <input
          ref={folder}
          type="file"
          multiple
          hidden
          data-testid="sto-pick-folder"
          {...{ webkitdirectory: '' }}
          onChange={(e) => {
            onFiles(e.target.files);
            e.target.value = '';
          }}
        />
      </div>
      {skipped > 0 && <p role="status">{t('storage.write.upload.skipped', { count: skipped })}</p>}
      {tasks.length > 0 && (
        <section className="sto-dock" aria-label={t('storage.write.dock.title')}>
          <h2 className="sto-dock-title">{t('storage.write.dock.title')}</h2>
          <progress
            value={pct(total)}
            max={100}
            aria-label={t('storage.write.dock.total', { percent: pct(total) })}
          />
          <ul className="sto-dock-list">
            {tasks.map((x) => (
              <li key={x.id}>
                <strong>{x.filename}</strong>
                <progress
                  value={pct(x.progress)}
                  max={100}
                  aria-label={t('storage.write.dock.row', {
                    name: x.filename,
                    percent: pct(x.progress),
                  })}
                />
                <span>
                  {stateText(x)}
                  {x.state === 'failed' ? `: ${taskErrorText(x)}` : ''}
                </span>
                <span className="us-actions">
                  {x.state === 'running' && (
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('storage.write.dock.pause', { name: x.filename })}
                      onClick={() => void m.pause(x.id)}
                    >
                      ❚❚
                    </Button>
                  )}
                  {(x.state === 'paused' || x.state === 'failed') && (
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t(
                        x.state === 'failed'
                          ? 'storage.write.dock.retry'
                          : 'storage.write.dock.resume',
                        { name: x.filename },
                      )}
                      onClick={() => void m.resume(x.id)}
                    >
                      ▶
                    </Button>
                  )}
                  {x.state !== 'done' && (
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('storage.write.dock.cancel', { name: x.filename })}
                      onClick={() => void m.cancel(x.id)}
                    >
                      ✕
                    </Button>
                  )}
                </span>
              </li>
            ))}
          </ul>
          {doneCount > 0 && (
            <Button size="sm" variant="ghost" onClick={() => m.clearFinished()}>
              {t('storage.write.dock.clear')}
            </Button>
          )}
        </section>
      )}
    </section>
  );
}
