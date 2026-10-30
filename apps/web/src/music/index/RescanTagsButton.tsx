import { Button } from '@sideroom/ui';
import { useCallback, useSyncExternalStore } from 'react';
import { useT } from '../../i18n';
import { getTagger, type Tagger } from './tagger';

/**
 * "Rescan tags" (MUS-IDX-001): marks every track of the mount for a tag read again and starts the background
 * tagger. Shown to whoever may write tags on the mount (`can_edit` in the library listing). The Full Library
 * Manager skin mounts it; it works on its own.
 */
export function RescanTagsButton({ mountId, tagger }: { mountId: string; tagger?: Tagger }) {
  const t = useT();
  const tg = tagger ?? getTagger();
  const subscribe = useCallback((fn: () => void) => tg.subscribe(fn), [tg]);
  const p = useSyncExternalStore(subscribe, () => tg.snapshot());
  return (
    <span className="mus-rescan">
      <Button
        type="button"
        disabled={p.running}
        onClick={() => {
          void tg.rescan(mountId).catch(() => undefined);
        }}
      >
        {t('music.index.rescan')}
      </Button>
      <span role="status" aria-live="polite">
        {p.running
          ? t('music.index.reading', { done: p.done, total: p.total })
          : p.failed > 0
            ? t('music.index.failed', { n: p.failed })
            : ''}
      </span>
    </span>
  );
}
