import { Button } from '@sideroom/ui';
import { useEffect, useRef, useState, type PointerEvent } from 'react';
import { useT } from '../i18n';
import type { Rect } from './covers';

export interface PictureSource {
  source: CanvasImageSource;
  w: number;
  h: number;
}

/** Smallest box (in picture pixels, after scaling to the preview) a drag must make to count as a cover. */
const MIN_DRAG_PX = 6;

const pct = (n: number, of: number) => `${(n / of) * 100}%`;

/**
 * The picture and its black boxes. Covers are in source pixels (what `renderJpeg` paints). A pointer drag on the
 * picture adds one; every cover has its own remove button, which a keyboard reaches by Tab.
 */
export function PictureEditor({
  picture,
  covers,
  onChange,
}: {
  picture: PictureSource;
  covers: Rect[];
  onChange: (next: Rect[]) => void;
}) {
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState<Rect | null>(null);
  const origin = useRef<{ x: number; y: number } | null>(null);

  // The preview is drawn once at a size the screen can hold; the page scales it with CSS.
  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const k = Math.min(1, 1280 / Math.max(picture.w, picture.h));
    c.width = Math.max(1, Math.round(picture.w * k));
    c.height = Math.max(1, Math.round(picture.h * k));
    try {
      c.getContext('2d')?.drawImage(picture.source, 0, 0, c.width, c.height);
    } catch {
      /* a canvas that cannot draw (a test) leaves the preview blank */
    }
  }, [picture]);

  const toPicture = (e: PointerEvent): { x: number; y: number } | null => {
    const r = stage.current?.getBoundingClientRect();
    if (!r || r.width <= 0 || r.height <= 0) return null;
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * picture.w;
    const y = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) * picture.h;
    return { x, y };
  };
  const rectOf = (a: { x: number; y: number }, b: { x: number; y: number }): Rect => ({
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.abs(a.x - b.x),
    h: Math.abs(a.y - b.y),
  });

  const down = (e: PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('.br-cover')) return;
    const p = toPicture(e);
    if (!p) return;
    origin.current = p;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDraft({ ...p, w: 0, h: 0 });
  };
  const move = (e: PointerEvent) => {
    if (!origin.current) return;
    const p = toPicture(e);
    if (p) setDraft(rectOf(origin.current, p));
  };
  const up = (e: PointerEvent) => {
    const o = origin.current;
    origin.current = null;
    if (!o) return;
    const p = toPicture(e);
    setDraft(null);
    if (!p) return;
    const r = rectOf(o, p);
    const scale = stage.current ? stage.current.getBoundingClientRect().width / picture.w : 1;
    if (r.w * scale >= MIN_DRAG_PX && r.h * scale >= MIN_DRAG_PX) onChange([...covers, r]);
  };

  return (
    <div
      ref={stage}
      className="br-stage"
      style={{
        aspectRatio: `${picture.w} / ${picture.h}`,
        // At most 22rem tall, so a tall picture does not push the form away.
        maxInlineSize: `${(22 * picture.w) / picture.h}rem`,
      }}
      data-testid="br-stage"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={() => {
        origin.current = null;
        setDraft(null);
      }}
    >
      <canvas
        ref={canvas}
        className="br-canvas"
        aria-label={t('bugreport.pictureLabel')}
        role="img"
      />
      {covers.map((c, i) => (
        <div
          key={i}
          className="br-cover"
          style={{
            left: pct(c.x, picture.w),
            top: pct(c.y, picture.h),
            width: pct(c.w, picture.w),
            height: pct(c.h, picture.h),
          }}
        >
          <Button
            size="sm"
            variant="secondary"
            className="br-cover-x"
            aria-label={t('bugreport.removeCover', { n: String(i + 1) })}
            onClick={() => onChange(covers.filter((_, j) => j !== i))}
          >
            ×
          </Button>
        </div>
      ))}
      {draft && (
        <div
          className="br-cover br-cover-draft"
          style={{
            left: pct(draft.x, picture.w),
            top: pct(draft.y, picture.h),
            width: pct(draft.w, picture.w),
            height: pct(draft.h, picture.h),
          }}
        />
      )}
    </div>
  );
}
