import { useEffect, useRef, useState } from 'react';
import type { PageEntry } from '../types';
import { decodeBlurhash } from './blurhash';
import { cropView, pickVariant } from './model';
import { cachedImageUrl } from './offlineCache';

/** Paints a blurhash into a small canvas; silently does nothing where canvas is unavailable. */
function BlurPlaceholder({ hash }: { hash: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || typeof ImageData === 'undefined') return;
    const px = decodeBlurhash(hash, 32, 32);
    const ctx = px ? canvas.getContext('2d') : null;
    if (px && ctx) ctx.putImageData(new ImageData(px, 32, 32), 0, 0);
  }, [hash]);
  return <canvas ref={ref} className="lib-page-blur" width={32} height={32} aria-hidden="true" />;
}

export interface PageImageProps {
  page: PageEntry;
  /** Pixel width the page will be shown at, for the variant choice. */
  cssWidth: number;
  /** Mounts the <img>; pages far from the viewport render only the placeholder. */
  active?: boolean;
  alt: string;
  /** Cut the borders found at ingest (LIB-ING-007), when the page has crop data. */
  crop?: boolean;
}

/** One page: blurhash placeholder at the page's own aspect ratio, the sized variant fading in over it. */
export function PageImage({ page, cssWidth, active = true, alt, crop = false }: PageImageProps) {
  const [loaded, setLoaded] = useState(false);
  // When the network image fails, a copy from the offline cache (LIB-RDR-015) stands in for it.
  const [offlineSrc, setOfflineSrc] = useState<string | null>(null);
  useEffect(
    () => () => {
      if (offlineSrc) URL.revokeObjectURL(offlineSrc);
    },
    [offlineSrc],
  );
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const src = pickVariant(page, cssWidth, dpr);
  const view = crop ? cropView(page, page.crop) : null;
  const fallback = async () => {
    if (offlineSrc) return;
    const urls = [src, ...page.variants.map((v) => v.url), page.url];
    for (const u of urls) {
      const hit = await cachedImageUrl(u);
      if (hit) return setOfflineSrc(hit);
    }
  };
  return (
    <div
      className="lib-page"
      data-loaded={loaded || undefined}
      data-cropped={view ? 'true' : undefined}
      style={{ aspectRatio: view ? view.ratio : `${page.width} / ${page.height}` }}
    >
      {page.blurhash && !loaded && <BlurPlaceholder hash={page.blurhash} />}
      {active && (
        <img
          className="lib-page-img"
          src={offlineSrc ?? src}
          alt={alt}
          width={page.width}
          height={page.height}
          decoding="async"
          draggable={false}
          onLoad={() => setLoaded(true)}
          onError={() => void fallback()}
          style={
            view
              ? {
                  position: 'absolute',
                  insetBlockStart: 0,
                  insetInlineStart: 0,
                  inlineSize: `${view.widthPct}%`,
                  maxInlineSize: 'none',
                  blockSize: 'auto',
                  transform: `translate(-${view.shiftX}%, -${view.shiftY}%)`,
                  transformOrigin: 'top left',
                }
              : undefined
          }
        />
      )}
    </div>
  );
}
