import { Button, Modal, Notice, Slider } from '@sideroom/ui';
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { cropWindow, loadImage, type LoadedImage } from './imageCrop';

export interface ImageCropDialogProps {
  open: boolean;
  /** The picked picture (PNG, JPEG or WebP; GIFs skip cropping). */
  file: File | null;
  title: string;
  /** Width / height of the crop window and of the saved picture. */
  aspect: number;
  /** Size in pixels of the saved picture. */
  outWidth: number;
  onCancel: () => void;
  /** The cropped picture as a data URI. */
  onApply: (dataUri: string) => void;
}

const VIEW_W = 280;
const STEP = 0.05;

/**
 * Crop step for avatar and banner uploads: drag (or use the arrow keys) to move the picture, slide to zoom.
 * The saved picture is drawn on a canvas at `outWidth`, JPEG for JPEG sources and PNG otherwise.
 */
export function ImageCropDialog({
  open,
  file,
  title,
  aspect,
  outWidth,
  onCancel,
  onApply,
}: ImageCropDialogProps) {
  const [img, setImg] = useState<LoadedImage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0.5, y: 0.5 });
  const [busy, setBusy] = useState(false);
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);

  useEffect(() => {
    setImg(null);
    setError(null);
    setZoom(1);
    setPan({ x: 0.5, y: 0.5 });
    if (!open || !file) return;
    let live = true;
    let loaded: LoadedImage | null = null;
    loadImage(file).then(
      (i) => {
        loaded = i;
        if (live) setImg(i);
        else i.release();
      },
      (e: unknown) =>
        live && setError(e instanceof Error ? e.message : 'Could not read the image.'),
    );
    return () => {
      live = false;
      loaded?.release();
    };
  }, [open, file]);

  const viewH = VIEW_W / aspect;
  const win = img ? cropWindow(img.width, img.height, aspect, zoom, pan.x, pan.y) : null;
  // Scale that maps the crop window onto the viewport; the picture is offset so the window lines up.
  const scale = win ? VIEW_W / win.sw : 1;

  const movePan = (dx: number, dy: number) =>
    setPan((p) => ({
      x: Math.min(1, Math.max(0, p.x + dx)),
      y: Math.min(1, Math.max(0, p.y + dy)),
    }));

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    drag.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || !img || !win) return;
    // Dragging right moves the picture right, so the window moves left in picture space.
    const spaceX = img.width - win.sw;
    const spaceY = img.height - win.sh;
    const nx = spaceX > 0 ? d.px - (e.clientX - d.x) / scale / spaceX : d.px;
    const ny = spaceY > 0 ? d.py - (e.clientY - d.y) / scale / spaceY : d.py;
    setPan({ x: Math.min(1, Math.max(0, nx)), y: Math.min(1, Math.max(0, ny)) });
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const k = e.key;
    if (k === 'ArrowLeft') movePan(-STEP, 0);
    else if (k === 'ArrowRight') movePan(STEP, 0);
    else if (k === 'ArrowUp') movePan(0, -STEP);
    else if (k === 'ArrowDown') movePan(0, STEP);
    else return;
    e.preventDefault();
  };

  const apply = () => {
    if (!img || !win || !file) return;
    setBusy(true);
    try {
      const mime = file.type === 'image/jpeg' ? 'image/jpeg' : 'image/png';
      onApply(img.render(win, outWidth, Math.round(outWidth / aspect), mime));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not crop the image.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      description="Drag the picture, or focus it and use the arrow keys, to choose what shows."
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button disabled={!img} loading={busy} onClick={apply}>
            Apply
          </Button>
        </>
      }
    >
      {error ? (
        <Notice variant="danger" title="Couldn't use that image">
          {error}
        </Notice>
      ) : !img || !win ? (
        <p className="us-muted" role="status">
          Loading the picture...
        </p>
      ) : (
        <div className="us-stack">
          <div
            className="us-crop"
            style={{ width: VIEW_W, height: viewH }}
            role="group"
            aria-label="Crop area"
            tabIndex={0}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={() => (drag.current = null)}
            onPointerCancel={() => (drag.current = null)}
            onKeyDown={onKey}
          >
            <img
              src={img.url}
              alt=""
              draggable={false}
              style={{
                width: img.width * scale,
                height: img.height * scale,
                left: -win.sx * scale,
                top: -win.sy * scale,
              }}
            />
          </div>
          <Slider
            label="Zoom"
            min={1}
            max={4}
            step={0.05}
            value={zoom}
            onValueChange={setZoom}
            showValue
            formatValue={(v) => `${Math.round(v * 100)}%`}
          />
        </div>
      )}
    </Modal>
  );
}
