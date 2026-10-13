import { useEffect, useRef, useState } from 'react';
import type { PageEntry } from '../types';
import { decodeBlurhash } from './blurhash';
import { pickVariant } from './model';

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
}

/** One page: blurhash placeholder at the page's own aspect ratio, the sized variant fading in over it. */
export function PageImage({ page, cssWidth, active = true, alt }: PageImageProps) {
  const [loaded, setLoaded] = useState(false);
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const src = pickVariant(page, cssWidth, dpr);
  return (
    <div
      className="lib-page"
      data-loaded={loaded || undefined}
      style={{ aspectRatio: `${page.width} / ${page.height}` }}
    >
      {page.blurhash && !loaded && <BlurPlaceholder hash={page.blurhash} />}
      {active && (
        <img
          className="lib-page-img"
          src={src}
          alt={alt}
          width={page.width}
          height={page.height}
          decoding="async"
          draggable={false}
          onLoad={() => setLoaded(true)}
        />
      )}
    </div>
  );
}
