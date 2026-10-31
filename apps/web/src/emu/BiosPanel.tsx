import { Button, Notice } from '@sideroom/ui';
import { useCallback, useEffect, useState } from 'react';
import { apiPost } from '../api/client';
import type { Mount } from '../settings/user/storage/types';
import { useT } from '../i18n';
import { fetchBios, KNOWN_PS1_BIOS, type BiosVerdict } from './bios';
import { listBiosFiles, type BiosCandidate } from './romApi';
import type { GameBios } from './useEmuGame';

/**
 * The PlayStation BIOS (ACT-EMU-001, note 6): the member picks a file from `emu/bios/` of their OWN bucket, the browser
 * reads it, checks its digest against the list of known dumps and keeps it in memory for this visit. The panel says why:
 * the BIOS is Sony's, so the instance never ships, stores or caches one.
 */
export function BiosPanel({
  mounts,
  userId,
  bios,
  onBios,
}: {
  mounts: readonly Mount[];
  userId: string;
  bios: GameBios | null;
  onBios: (b: GameBios | null) => void;
}) {
  const t = useT();
  const [files, setFiles] = useState<BiosCandidate[] | null>(null);
  const [verdict, setVerdict] = useState<{ key: string; v: BiosVerdict } | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    void listBiosFiles(mounts).then((f) => live && setFiles(f));
    return () => {
      live = false;
    };
  }, [mounts]);

  const use = useCallback(
    async (c: BiosCandidate) => {
      setBusy(true);
      setFailed(false);
      try {
        const r = await fetchBios(
          c.mountId,
          { mountOwnerKind: 'user', mountOwnerId: userId, key: c.key },
          userId,
          {
            presignGet: (m, k) =>
              apiPost<{ url: string }>(`/storage/mounts/${encodeURIComponent(m)}/presign`, {
                key: k,
                method: 'GET',
              }),
            fetch: (...a) => fetch(...a),
          },
        );
        setVerdict({ key: c.key, v: r.verdict });
        if (r.verdict.ok && r.bytes) onBios({ file: r.verdict.entry.file, bytes: r.bytes });
        else onBios(null);
      } catch {
        setFailed(true);
        onBios(null);
      } finally {
        setBusy(false);
      }
    },
    [onBios, userId],
  );

  return (
    <section className="emu-bios" aria-labelledby="emu-bios-h">
      <h3 id="emu-bios-h">{t('games.bios.title')}</h3>
      <p>{t('games.bios.why')}</p>
      <p>{t('games.bios.where')}</p>
      {bios && (
        <Notice variant="success" title={t('games.bios.accepted', { file: bios.file })}>
          {t('games.bios.memoryOnly')}{' '}
          <Button variant="link" onClick={() => onBios(null)}>
            {t('games.bios.forget')}
          </Button>
        </Notice>
      )}
      {failed && <Notice variant="danger" title={t('games.bios.readFailed')} />}
      {verdict && !verdict.v.ok && (
        <Notice variant="warning" title={t('games.bios.refused')}>
          {verdict.v.reason === 'size'
            ? t('games.bios.refusedSize')
            : t('games.bios.refusedUnknown', {
                count: KNOWN_PS1_BIOS.length,
                sha1: verdict.v.sha1 ?? '',
              })}
        </Notice>
      )}
      {files && files.length === 0 && <p>{t('games.bios.none')}</p>}
      <ul className="emu-bios-list">
        {(files ?? []).map((c) => (
          <li key={`${c.mountId}:${c.key}`}>
            <span>{c.key.slice('emu/bios/'.length)}</span>{' '}
            <span className="emu-dim">{c.mountLabel}</span>{' '}
            <Button variant="secondary" size="sm" disabled={busy} onClick={() => void use(c)}>
              {t('games.bios.use')}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
