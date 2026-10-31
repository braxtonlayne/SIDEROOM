// Canvas painting for the whiteboard. Everything is drawn in page coordinates (1600 x 900); the caller scales
// the context. The same function paints the screen and the PNG export.

import { PAGE_H, PAGE_W, type DrawObject } from './log';

export type ImageCache = Map<string, HTMLImageElement | 'loading' | 'failed'>;

function arrowHead(
  ctx: CanvasRenderingContext2D,
  a: readonly [number, number],
  b: readonly [number, number],
  w: number,
) {
  const ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
  const len = Math.max(12, w * 4);
  ctx.beginPath();
  ctx.moveTo(b[0], b[1]);
  ctx.lineTo(b[0] - len * Math.cos(ang - Math.PI / 6), b[1] - len * Math.sin(ang - Math.PI / 6));
  ctx.moveTo(b[0], b[1]);
  ctx.lineTo(b[0] - len * Math.cos(ang + Math.PI / 6), b[1] - len * Math.sin(ang + Math.PI / 6));
  ctx.stroke();
}

export function drawObject(
  ctx: CanvasRenderingContext2D,
  o: DrawObject,
  images: ImageCache,
  onImage: () => void,
): void {
  ctx.save();
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  switch (o.k) {
    case 'stroke': {
      ctx.strokeStyle = o.c;
      ctx.fillStyle = o.c;
      ctx.lineWidth = o.w;
      if (o.hl) {
        ctx.globalAlpha = 0.35;
        ctx.lineCap = 'butt';
      }
      const p = o.pts;
      if (p.length === 2) {
        ctx.beginPath();
        ctx.arc(p[0] as number, p[1] as number, o.w / 2, 0, Math.PI * 2);
        ctx.fill();
        break;
      }
      ctx.beginPath();
      ctx.moveTo(p[0] as number, p[1] as number);
      for (let i = 2; i + 3 < p.length; i += 2) {
        const mx = ((p[i] as number) + (p[i + 2] as number)) / 2;
        const my = ((p[i + 1] as number) + (p[i + 3] as number)) / 2;
        ctx.quadraticCurveTo(p[i] as number, p[i + 1] as number, mx, my);
      }
      ctx.lineTo(p[p.length - 2] as number, p[p.length - 1] as number);
      ctx.stroke();
      break;
    }
    case 'shape': {
      ctx.strokeStyle = o.c;
      ctx.fillStyle = o.c;
      ctx.lineWidth = o.w;
      const x = Math.min(o.a[0], o.b[0]);
      const y = Math.min(o.a[1], o.b[1]);
      const w = Math.abs(o.b[0] - o.a[0]);
      const h = Math.abs(o.b[1] - o.a[1]);
      if (o.s === 'rect') {
        if (o.fill) ctx.fillRect(x, y, w, h);
        else ctx.strokeRect(x, y, w, h);
      } else if (o.s === 'ellipse') {
        ctx.beginPath();
        ctx.ellipse(
          x + w / 2,
          y + h / 2,
          Math.max(0.5, w / 2),
          Math.max(0.5, h / 2),
          0,
          0,
          Math.PI * 2,
        );
        if (o.fill) ctx.fill();
        else ctx.stroke();
      } else {
        ctx.beginPath();
        ctx.moveTo(o.a[0], o.a[1]);
        ctx.lineTo(o.b[0], o.b[1]);
        ctx.stroke();
        if (o.s === 'arrow') arrowHead(ctx, o.a, o.b, o.w);
      }
      break;
    }
    case 'text': {
      ctx.fillStyle = o.c;
      ctx.font = `${o.size}px system-ui, sans-serif`;
      ctx.textBaseline = 'top';
      o.text
        .split('\n')
        .forEach((line, i) => ctx.fillText(line, o.at[0], o.at[1] + i * o.size * 1.25));
      break;
    }
    case 'image': {
      if (o.src === '') {
        // Its parts have not all arrived yet.
        ctx.strokeStyle = '#8a8a8a';
        ctx.setLineDash([6, 6]);
        ctx.strokeRect(o.at[0], o.at[1], o.size[0], o.size[1]);
        break;
      }
      let img = images.get(o.src);
      if (img === undefined) {
        const el = new Image();
        images.set(o.src, 'loading');
        el.onload = () => {
          images.set(o.src, el);
          onImage();
        };
        el.onerror = () => {
          images.set(o.src, 'failed');
          onImage();
        };
        el.src = o.src;
        img = 'loading';
      }
      if (img !== 'loading' && img !== 'failed') {
        ctx.drawImage(img, o.at[0], o.at[1], o.size[0], o.size[1]);
      } else {
        ctx.strokeStyle = '#8a8a8a';
        ctx.setLineDash([6, 6]);
        ctx.strokeRect(o.at[0], o.at[1], o.size[0], o.size[1]);
      }
      break;
    }
  }
  ctx.restore();
}

export function drawPage(
  ctx: CanvasRenderingContext2D,
  objects: DrawObject[],
  pageId: string,
  images: ImageCache,
  onImage: () => void,
  background: string | null = '#ffffff',
): void {
  if (background) {
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, PAGE_W, PAGE_H);
  }
  for (const o of objects) if (o.pg === pageId) drawObject(ctx, o, images, onImage);
}
