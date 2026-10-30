/**
 * UTL-IMG-001: meme captions. Top and bottom text in a stroked style (white fill, dark outline), set in one of
 * the fonts of the token set. The layout is pure over a `measure` function so it is tested without a canvas;
 * `drawCaptions` paints onto any 2D context.
 */

export type CaptionFont = 'display' | 'body' | 'mono';

/** The CSS custom properties that hold the theme's fonts (packages/ui tokens). */
export const FONT_VARS: Record<CaptionFont, string> = {
  display: '--ui-font-display',
  body: '--ui-font-body',
  mono: '--ui-font-mono',
};

const FALLBACK: Record<CaptionFont, string> = {
  display: 'Georgia, serif',
  body: 'system-ui, sans-serif',
  mono: 'ui-monospace, monospace',
};

/** The font family of a token-set font, read from the document (a fallback stack when unset, as in tests). */
export function resolveFont(font: CaptionFont, root?: Element): string {
  try {
    const el = root ?? (typeof document !== 'undefined' ? document.documentElement : undefined);
    const v = el ? getComputedStyle(el).getPropertyValue(FONT_VARS[font]).trim() : '';
    return v || FALLBACK[font];
  } catch {
    return FALLBACK[font];
  }
}

export interface CaptionOptions {
  top: string;
  bottom: string;
  font: CaptionFont;
  /** 0.5 to 2; 1 is a tenth of the image width. */
  size: number;
  uppercase: boolean;
  fill: string;
  stroke: string;
}

export const DEFAULT_CAPTION: CaptionOptions = {
  top: '',
  bottom: '',
  font: 'body',
  size: 1,
  uppercase: true,
  fill: '#ffffff',
  stroke: '#000000',
};

/** Greedy word wrap; a word wider than the line is broken by characters. */
export function wrapLines(
  text: string,
  maxWidth: number,
  measure: (s: string) => number,
): string[] {
  const lines: string[] = [];
  for (const para of text.split(/\r?\n/)) {
    let line = '';
    for (const word of para.split(/\s+/).filter(Boolean)) {
      const next = line ? `${line} ${word}` : word;
      if (measure(next) <= maxWidth) {
        line = next;
        continue;
      }
      if (line) lines.push(line);
      line = '';
      let rest = word;
      while (rest && measure(rest) > maxWidth) {
        let n = rest.length - 1;
        while (n > 1 && measure(rest.slice(0, n)) > maxWidth) n--;
        lines.push(rest.slice(0, n));
        rest = rest.slice(n);
      }
      line = rest;
    }
    if (line || para.trim() === '') lines.push(line);
  }
  while (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

export interface CaptionBlock {
  lines: string[];
  fontPx: number;
  lineHeight: number;
}

/** Fit a caption: start at the requested size and shrink until it takes at most 40% of the height. */
export function layoutCaption(
  text: string,
  width: number,
  height: number,
  size: number,
  measureAt: (fontPx: number) => (s: string) => number,
): CaptionBlock | null {
  if (!text.trim()) return null;
  let fontPx = Math.max(10, Math.round((width / 10) * Math.min(2, Math.max(0.5, size))));
  const maxWidth = width * 0.92;
  for (;;) {
    const lines = wrapLines(text, maxWidth, measureAt(fontPx));
    const lineHeight = Math.round(fontPx * 1.1);
    if (lines.length * lineHeight <= height * 0.4 || fontPx <= 10) {
      return { lines, fontPx, lineHeight };
    }
    fontPx = Math.max(10, Math.floor(fontPx * 0.9));
  }
}

/** The subset of CanvasRenderingContext2D the captions use. */
export interface TextContext {
  font: string;
  fillStyle: unknown;
  strokeStyle: unknown;
  lineWidth: number;
  lineJoin: string;
  textAlign: string;
  textBaseline: string;
  measureText(text: string): { width: number };
  fillText(text: string, x: number, y: number): void;
  strokeText(text: string, x: number, y: number): void;
}

/** Paint the top and bottom captions onto `ctx` for an image of `width` x `height`. Returns the lines drawn. */
export function drawCaptions(
  ctx: TextContext,
  width: number,
  height: number,
  o: CaptionOptions,
  family: string = resolveFont(o.font),
): { top: string[]; bottom: string[] } {
  const fontOf = (px: number) => `800 ${px}px ${family}`;
  const measureAt = (px: number) => (s: string) => {
    ctx.font = fontOf(px);
    return ctx.measureText(s).width;
  };
  const prep = (s: string) => (o.uppercase ? s.toUpperCase() : s);
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';
  ctx.lineJoin = 'round';
  ctx.fillStyle = o.fill;
  ctx.strokeStyle = o.stroke;
  const out = { top: [] as string[], bottom: [] as string[] };

  const paint = (block: CaptionBlock, startY: number) => {
    ctx.font = fontOf(block.fontPx);
    ctx.lineWidth = Math.max(2, block.fontPx / 7);
    block.lines.forEach((line, i) => {
      const y = startY + i * block.lineHeight;
      ctx.strokeText(line, width / 2, y);
      ctx.fillText(line, width / 2, y);
    });
  };

  const top = layoutCaption(prep(o.top), width, height, o.size, measureAt);
  if (top) {
    paint(top, Math.round(height * 0.02));
    out.top = top.lines;
  }
  const bottom = layoutCaption(prep(o.bottom), width, height, o.size, measureAt);
  if (bottom) {
    paint(bottom, Math.round(height * 0.98) - bottom.lines.length * bottom.lineHeight);
    out.bottom = bottom.lines;
  }
  return out;
}
