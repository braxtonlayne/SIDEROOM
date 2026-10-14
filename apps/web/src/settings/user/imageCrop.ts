/**
 * Image picking and cropping for avatar and banner uploads (the server takes `data:image/...` URIs of at most
 * 8 MiB: `PATCH /users/@me {avatar, banner}`). The cropping maths is pure so it can be tested without a canvas.
 */

export const MAX_IMAGE_BYTES = 8 * 1024 * 1024;
export const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'] as const;

/** Why a file cannot be used, or null when it can. */
export function imageProblem(file: Pick<File, 'type' | 'size'>): string | null {
  if (!(IMAGE_TYPES as readonly string[]).includes(file.type))
    return 'Choose a PNG, JPEG, GIF or WebP image.';
  if (file.size > MAX_IMAGE_BYTES) return 'That image is larger than 8 MiB.';
  return null;
}

export interface CropWindow {
  sx: number;
  sy: number;
  sw: number;
  sh: number;
}

/**
 * The part of a `width` x `height` picture that is visible through a window of the given aspect ratio
 * (width / height) at `zoom` (1 = the largest window that fits). `panX` and `panY` are 0..1 and say where in
 * the leftover space the window sits (0.5 = centred).
 */
export function cropWindow(
  width: number,
  height: number,
  aspect: number,
  zoom: number,
  panX: number,
  panY: number,
): CropWindow {
  let sw = width;
  let sh = width / aspect;
  if (sh > height) {
    sh = height;
    sw = height * aspect;
  }
  const z = Math.max(1, zoom);
  sw /= z;
  sh /= z;
  const px = Math.min(1, Math.max(0, panX));
  const py = Math.min(1, Math.max(0, panY));
  return { sx: (width - sw) * px, sy: (height - sh) * py, sw, sh };
}

export interface LoadedImage {
  width: number;
  height: number;
  /** Object URL for the preview. */
  url: string;
  release: () => void;
  /** Draws `win` of the picture into a canvas of `outW` x `outH` and returns it as a data URI. */
  render: (win: CropWindow, outW: number, outH: number, mime: string) => string;
}

/** Decodes a picked file. Rejects when the browser cannot read it as an image. */
export function loadImage(file: File): Promise<LoadedImage> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () =>
      resolve({
        width: img.naturalWidth,
        height: img.naturalHeight,
        url,
        release: () => URL.revokeObjectURL(url),
        render: (win, outW, outH, mime) => {
          const canvas = document.createElement('canvas');
          canvas.width = outW;
          canvas.height = outH;
          const ctx = canvas.getContext('2d');
          if (!ctx) throw new Error('Canvas is not available in this browser.');
          ctx.imageSmoothingQuality = 'high';
          ctx.drawImage(img, win.sx, win.sy, win.sw, win.sh, 0, 0, outW, outH);
          return canvas.toDataURL(mime, 0.92);
        },
      });
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('That file could not be read as an image.'));
    };
    img.src = url;
  });
}

/** The file as a `data:` URI, untouched (used for animated GIFs, which a canvas would flatten). */
export function readAsDataUri(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error('That file could not be read.'));
    r.readAsDataURL(file);
  });
}
