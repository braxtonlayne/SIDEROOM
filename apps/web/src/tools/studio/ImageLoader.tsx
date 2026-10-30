/**
 * UTL-IMG-001: gets an image into the studio: a file picker, a drop target and paste. Everything is read in the
 * page (createImageBitmap or an object URL); nothing is uploaded. `source` lets another surface (the composer's
 * attachment, a message image) start the studio with a file already chosen.
 */
import { Button, Icon } from '@sideroom/ui';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ClipboardEvent,
  type DragEvent,
  type ReactNode,
} from 'react';
import { useT } from '../../i18n';
import type { Bitmap } from './bitmap';
import { decodeImage } from './canvasIo';
import './studio.css';

export function ImageLoader({
  source,
  children,
}: {
  source?: Blob | undefined;
  children: (bitmap: Bitmap, baseName: string) => ReactNode;
}) {
  const t = useT();
  const [loaded, setLoaded] = useState<{ bitmap: Bitmap; name: string } | null>(null);
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const load = useCallback(async (blob: Blob, name: string) => {
    setBusy(true);
    setError(false);
    try {
      const bitmap = await decodeImage(blob);
      setLoaded({ bitmap, name: name.replace(/\.[^.]+$/, '') || 'image' });
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (source) void load(source, source instanceof File ? source.name : 'image');
  }, [source, load]);

  const take = (files: FileList | File[] | null | undefined) => {
    const f = Array.from(files ?? []).find((x) => x.type.startsWith('image/'));
    if (f) void load(f, f.name);
    else if (files && files.length) setError(true);
  };

  if (loaded) return <>{children(loaded.bitmap, loaded.name)}</>;
  return (
    <div
      className="sr-st-pick"
      tabIndex={0}
      onPaste={(e: ClipboardEvent) => take(e.clipboardData?.files)}
      onDragOver={(e: DragEvent) => e.preventDefault()}
      onDrop={(e: DragEvent) => {
        e.preventDefault();
        take(e.dataTransfer?.files);
      }}
    >
      <Icon name="image" size={24} />
      <p>{busy ? t('studio.pick.loading') : t('studio.pick.hint')}</p>
      <input
        ref={input}
        type="file"
        accept="image/*"
        hidden
        data-testid="studio-file"
        aria-label={t('studio.pick.title')}
        onChange={(e) => {
          take(e.target.files);
          e.target.value = '';
        }}
      />
      <Button size="sm" onClick={() => input.current?.click()} disabled={busy}>
        {t('studio.pick.button')}
      </Button>
      {error && (
        <p role="alert" className="sr-st-error">
          {t('studio.pick.invalid')}
        </p>
      )}
    </div>
  );
}
