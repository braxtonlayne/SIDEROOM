/**
 * UTL-IMG-001: an editing session. One working bitmap plus an undo history; every operation replaces the
 * working bitmap with its result, so what is exported is always the edited pixels (redacted areas are
 * overwritten in the working bitmap, never overlaid). The session does no I/O: it never fetches, posts or
 * stores anything, and `exportAs` only encodes the working bitmap.
 */
import {
  cloneBitmap,
  crop,
  flip,
  pad,
  padToAspect,
  pixelate,
  redact,
  resize,
  rotate90,
  type Bitmap,
  type Padding,
  type Region,
  type Rgba,
} from './bitmap';
import type { CaptionOptions } from './caption';
import { applyCaptions, exportBitmap, type ExportFormat } from './canvasIo';

const HISTORY_LIMIT = 20;

export class StudioSession {
  private past: Bitmap[] = [];
  private future: Bitmap[] = [];
  private bitmap: Bitmap;

  constructor(initial: Bitmap) {
    this.bitmap = initial;
  }

  /** The working bitmap (do not mutate it from outside). */
  get current(): Bitmap {
    return this.bitmap;
  }
  get canUndo(): boolean {
    return this.past.length > 0;
  }
  get canRedo(): boolean {
    return this.future.length > 0;
  }

  private commit(next: Bitmap): void {
    this.past.push(this.bitmap);
    if (this.past.length > HISTORY_LIMIT) this.past.shift();
    this.future = [];
    this.bitmap = next;
  }

  undo(): void {
    const prev = this.past.pop();
    if (!prev) return;
    this.future.push(this.bitmap);
    this.bitmap = prev;
  }
  redo(): void {
    const next = this.future.pop();
    if (!next) return;
    this.past.push(this.bitmap);
    this.bitmap = next;
  }

  /** Forget the history. After this, "undo" cannot bring back what a redaction covered. */
  forgetHistory(): void {
    this.past = [];
    this.future = [];
  }

  crop(region: Region): void {
    this.commit(crop(this.bitmap, region));
  }
  resize(width: number, height: number): void {
    this.commit(resize(this.bitmap, width, height));
  }
  pad(p: Padding, color?: Rgba): void {
    this.commit(pad(this.bitmap, p, color));
  }
  padToAspect(ratio: number, color?: Rgba): void {
    this.commit(padToAspect(this.bitmap, ratio, color));
  }
  rotate(clockwise = true): void {
    this.commit(rotate90(this.bitmap, clockwise));
  }
  flip(horizontal: boolean): void {
    this.commit(flip(this.bitmap, horizontal));
  }
  caption(options: CaptionOptions): void {
    this.commit(applyCaptions(this.bitmap, options));
  }
  /** Cover a region with a solid colour. */
  redact(region: Region, color?: [number, number, number]): boolean {
    const next = cloneBitmap(this.bitmap);
    if (!redact(next, region, color)) return false;
    this.commit(next);
    return true;
  }
  /** Pixelate a region. */
  pixelate(region: Region, block: number): boolean {
    const next = cloneBitmap(this.bitmap);
    if (!pixelate(next, region, block)) return false;
    this.commit(next);
    return true;
  }

  exportAs(format: ExportFormat, quality?: number): Promise<Blob> {
    return exportBitmap(this.bitmap, format, quality);
  }
}
