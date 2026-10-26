import { Notice, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { friendsOf, useSocial } from '../../../lounge/shell/social';
import { useT } from '../../../i18n';
import { storageErrorText } from './errors';
import { deleteShare, listMounts, listShares, putShare } from './api';
import './storage.css';
import { SHARE_KINDS, type Mount, type ShareKind } from './types';

type Flags = Record<ShareKind, boolean>;
const NONE: Flags = { cinema: false, library: false, music: false };

/**
 * "Share my Media Shelf: Cinema, Library, Music" on a friend's profile (BYOS-UI-001). Shows for mutual
 * friends only, and only when the viewer has an active mount; the server refuses anything else and
 * revokes every share when the friendship ends.
 */
export function FriendShelfShare({
  userId,
  showEmpty = false,
}: {
  userId: string;
  /** Say why nothing shows (no active bucket) instead of rendering nothing. */
  showEmpty?: boolean;
}) {
  const t = useT();
  const rows = useSocial((s) => s.relationships);
  const isFriend = friendsOf(rows).some((r) => r.id === userId);
  const [mount, setMount] = useState<Mount | null>(null);
  const [flags, setFlags] = useState<Flags>(NONE);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!isFriend) return;
    let live = true;
    void (async () => {
      try {
        const m = (await listMounts({ kind: 'user' })).find((x) => x.status === 'active');
        if (!live || !m) return;
        const s = (await listShares(m.id)).find((x) => x.friend_id === userId);
        if (!live) return;
        setMount(m);
        setFlags(s ? { cinema: s.cinema, library: s.library, music: s.music } : NONE);
      } catch {
        /* no shelf, or not available yet: the toggle simply does not show */
      } finally {
        if (live) setLoaded(true);
      }
    })();
    return () => {
      live = false;
    };
  }, [isFriend, userId]);

  if (!isFriend) return null;
  if (!mount)
    return showEmpty && loaded ? (
      <p className="us-muted">{t('settings.storageShare.noBucket')}</p>
    ) : null;

  const toggle = async (kind: ShareKind, on: boolean) => {
    const next = { ...flags, [kind]: on };
    setBusy(true);
    setError(null);
    try {
      if (next.cinema || next.library || next.music) await putShare(mount.id, userId, next);
      else await deleteShare(mount.id, userId);
      setFlags(next);
    } catch (e) {
      setError(storageErrorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="sto-share" aria-labelledby="sto-share-h">
      <h3 id="sto-share-h" className="sto-share-title">
        {t('settings.storageShare.title')}
      </h3>
      {error && (
        <Notice variant="danger" title={t('settings.storageShare.failed')}>
          {error}
        </Notice>
      )}
      {SHARE_KINDS.map((k) => (
        <Switch
          key={k}
          label={t(`settings.storageShare.kind.${k}`)}
          checked={flags[k]}
          disabled={busy}
          onChange={(e) => void toggle(k, e.target.checked)}
        />
      ))}
    </section>
  );
}
