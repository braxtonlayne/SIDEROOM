import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { listMounts } from '../../settings/user/storage/api';
import type { Mount } from '../../settings/user/storage/types';
import { listObjects, type ObjectPage } from '../../storage/api';
import type { ImgRef } from './model';

const IMAGE = /\.(png|jpe?g|gif|webp|avif)$/i;
const baseName = (key: string) => key.split('/').filter(Boolean).pop() ?? key;

/**
 * Pick an image from one of the member's own buckets. Only the mount id and the object key go into the room;
 * every viewer then asks for their own link (see `RoomImage`).
 */
export function MountImagePicker({
  onPick,
  onClose,
}: {
  onPick: (img: ImgRef, label: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [mounts, setMounts] = useState<Mount[] | null>(null);
  const [mount, setMount] = useState('');
  const [prefix, setPrefix] = useState('');
  const [page, setPage] = useState<ObjectPage | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let dead = false;
    listMounts({ kind: 'user' }).then(
      (m) => !dead && setMounts(m),
      () => !dead && (setMounts([]), setFailed(true)),
    );
    return () => {
      dead = true;
    };
  }, []);

  useEffect(() => {
    if (!mount) return;
    let dead = false;
    setPage(null);
    setFailed(false);
    listObjects(mount, prefix).then(
      (p) => !dead && setPage(p),
      () => !dead && setFailed(true),
    );
    return () => {
      dead = true;
    };
  }, [mount, prefix]);

  return (
    <div className="sr-room-picker" role="group" aria-label={t('tools.rooms.mountPicker')}>
      <label className="sr-room-field">
        <span>{t('tools.rooms.mount')}</span>
        <select
          value={mount}
          onChange={(e) => {
            setMount(e.target.value);
            setPrefix('');
          }}
        >
          <option value="">{t('tools.rooms.pickMount')}</option>
          {mounts?.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      {mounts?.length === 0 && <p className="sr-tl-muted">{t('tools.rooms.noMounts')}</p>}
      {failed && (
        <p role="alert" className="sr-room-error">
          {t('tools.rooms.mountError')}
        </p>
      )}
      {mount && page && (
        <ul className="sr-room-files">
          {prefix && (
            <li>
              <button
                type="button"
                className="sr-room-btn"
                onClick={() => setPrefix(prefix.replace(/[^/]+\/$/, ''))}
              >
                {t('tools.rooms.up')}
              </button>
            </li>
          )}
          {page.folders.map((f) => (
            <li key={f}>
              <button type="button" className="sr-room-btn" onClick={() => setPrefix(f)}>
                {baseName(f)}/
              </button>
            </li>
          ))}
          {page.objects
            .filter((o) => IMAGE.test(o.key))
            .map((o) => (
              <li key={o.key}>
                <button
                  type="button"
                  className="sr-room-btn"
                  onClick={() => onPick({ k: 'mount', mount, key: o.key }, baseName(o.key))}
                >
                  {baseName(o.key)}
                </button>
              </li>
            ))}
        </ul>
      )}
      <button type="button" className="sr-room-btn" onClick={onClose}>
        {t('tools.rooms.cancel')}
      </button>
    </div>
  );
}
