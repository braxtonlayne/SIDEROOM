/**
 * Tab title and favicon unread badge (LNG-NTF-008). The count is the number of unread mentions; the title
 * gets a "(3) " prefix and the favicon a red dot. Turning the count back to 0 restores both.
 */
const PREFIX = /^\(\d+\+?\) /;

export function titleWithBadge(title: string, count: number): string {
  const base = title.replace(PREFIX, '');
  if (count <= 0) return base;
  return `(${count > 99 ? '99+' : count}) ${base}`;
}

let originalHref: string | null = null;

function iconLink(): HTMLLinkElement | null {
  return document.querySelector<HTMLLinkElement>('link[rel~="icon"]');
}

/** Draw `base` with a red dot on a canvas; null when canvas is unavailable. */
function dotted(base: HTMLImageElement): string | null {
  try {
    const size = 64;
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.drawImage(base, 0, 0, size, size);
    ctx.fillStyle = '#e5484d';
    ctx.beginPath();
    ctx.arc(size - 16, 16, 14, 0, Math.PI * 2);
    ctx.fill();
    return canvas.toDataURL('image/png');
  } catch {
    return null;
  }
}

export function setTabBadge(count: number): void {
  if (typeof document === 'undefined') return;
  document.title = titleWithBadge(document.title, count);
  const link = iconLink();
  if (!link) return;
  if (count <= 0) {
    if (originalHref !== null) {
      link.href = originalHref;
      originalHref = null;
    }
    return;
  }
  originalHref ??= link.href;
  const src = originalHref;
  const img = new Image();
  img.onload = () => {
    const url = dotted(img);
    if (url && originalHref !== null) link.href = url;
  };
  img.src = src;
}
