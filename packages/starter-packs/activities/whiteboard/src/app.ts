// The whiteboard's interface. It owns the DOM of index.html and talks to a `Board` only.
// Nothing here uses alert/confirm/prompt: the Activity sandbox has no `allow-modals`.

import { deliverPng, renderPagePng } from './export';
import { packStroke, pick } from './geom';
import { fileToImage, fitImage, urlToImage, type ImageResult } from './images';
import {
  HOME_PAGE,
  PAGE_H,
  PAGE_W,
  redoTarget,
  undoTarget,
  type DrawObject,
  type Pt,
  type ShapeKind,
} from './log';
import { drawObject, drawPage, type ImageCache } from './render';
import { S } from './strings';
import type { Board } from './sync';

type Tool =
  'pen' | 'highlighter' | 'eraser' | 'line' | 'arrow' | 'rect' | 'ellipse' | 'text' | 'image';
const TOOLS: Tool[] = [
  'pen',
  'highlighter',
  'eraser',
  'line',
  'arrow',
  'rect',
  'ellipse',
  'text',
  'image',
];
const COLORS = [
  '#111111',
  '#e03131',
  '#f08c00',
  '#2f9e44',
  '#1971c2',
  '#7048e8',
  '#c2255c',
  '#868e96',
];

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & Record<string, unknown> = {},
  children: Array<Node | string> = [],
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k.startsWith('aria-') || k.startsWith('data-')) e.setAttribute(k, String(v));
    else (e as unknown as Record<string, unknown>)[k] = v;
  }
  for (const c of children) e.append(c);
  return e;
}

export interface AppOptions {
  /** A line shown in the status area (the stand-alone notice). */
  note?: string;
}

export function startApp(board: Board, opts: AppOptions = {}): () => void {
  const canvas = $<HTMLCanvasElement>('board');
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas');
  const images: ImageCache = new Map();
  const toolsEl = $('tools');
  const colorsEl = $('colors');
  const widthEl = $<HTMLInputElement>('width');
  const fillEl = $<HTMLInputElement>('fill');
  const statusEl = $('status');
  const textEl = $<HTMLTextAreaElement>('text-input');
  const pagesEl = $('pages');
  const fileEl = $<HTMLInputElement>('file');
  const undoBtn = $<HTMLButtonElement>('undo');
  const redoBtn = $<HTMLButtonElement>('redo');
  const clearBtn = $<HTMLButtonElement>('clear');
  const exportBtn = $<HTMLButtonElement>('export');
  const addPageBtn = $<HTMLButtonElement>('page-add');
  const renameBtn = $<HTMLButtonElement>('page-rename');
  const delPageBtn = $<HTMLButtonElement>('page-del');

  let tool: Tool = 'pen';
  let color = COLORS[0] as string;
  let pageId = HOME_PAGE;
  let draft: DrawObject | null = null;
  let raw: Pt[] = [];
  let start: Pt | null = null;
  let pendingText: Pt | null = null;
  let pendingImageAt: Pt | null = null;
  let status = opts.note ?? '';
  let raf = 0;
  let editingPage = false;
  let canUndo = false;
  let canRedo = false;

  // --- static labels ---------------------------------------------------------------------------------------
  $('width-label').textContent = S.width;
  $('fill-label').textContent = S.fill;
  undoBtn.textContent = S.undo;
  redoBtn.textContent = S.redo;
  clearBtn.textContent = S.clear;
  exportBtn.textContent = S.exportPng;
  addPageBtn.textContent = S.pageAdd;
  renameBtn.textContent = S.pageRename;
  delPageBtn.textContent = S.pageDelete;
  textEl.placeholder = S.textPlaceholder;
  textEl.setAttribute('aria-label', S.tools.text);
  document.title = S.title;

  // Image chooser row (shown with the image tool).
  const urlInput = el('input', { type: 'url', placeholder: 'https://...', id: 'image-url' });
  urlInput.setAttribute('aria-label', S.imageUrlPrompt);
  const urlAdd = el('button', { type: 'button', textContent: S.tools.image });
  const fileBtn = el('button', { type: 'button', textContent: '...' });
  fileBtn.setAttribute('aria-label', S.imageUrlPrompt);
  const imageRow = el('div', { className: 'wb-group', hidden: true }, [urlInput, urlAdd, fileBtn]);
  $('tools').after(imageRow);

  const buttons = new Map<Tool, HTMLButtonElement>();
  for (const t of TOOLS) {
    const b = el('button', { type: 'button', textContent: S.tools[t] });
    b.setAttribute('role', 'radio');
    b.dataset.tool = t;
    b.addEventListener('click', () => setTool(t));
    toolsEl.append(b);
    buttons.set(t, b);
  }
  const swatches: HTMLButtonElement[] = [];
  for (const c of COLORS) {
    const b = el('button', { type: 'button', className: 'wb-swatch' });
    b.style.backgroundColor = c;
    b.setAttribute('role', 'radio');
    b.setAttribute('aria-label', `${S.colour} ${c}`);
    b.dataset.color = c;
    b.addEventListener('click', () => {
      color = c;
      syncChrome();
    });
    colorsEl.append(b);
    swatches.push(b);
  }

  function setTool(t: Tool): void {
    commitText();
    tool = t;
    imageRow.hidden = t !== 'image';
    syncChrome();
  }

  // --- drawing helpers -------------------------------------------------------------------------------------
  function toPage(ev: PointerEvent | MouseEvent): Pt {
    const r = canvas.getBoundingClientRect();
    return [
      Math.min(PAGE_W, Math.max(0, ((ev.clientX - r.left) / r.width) * PAGE_W)),
      Math.min(PAGE_H, Math.max(0, ((ev.clientY - r.top) / r.height) * PAGE_H)),
    ];
  }
  const width = (): number => Number(widthEl.value) || 4;
  const widthFor = (t: Tool): number => (t === 'highlighter' ? width() * 4 : width());
  const rounded = (p: Pt): Pt => [Math.round(p[0]), Math.round(p[1])];

  function schedule(): void {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      paint();
    });
  }

  function paint(): void {
    if (!ctx) return;
    const view = board.view();
    if (!view.pages.some((p) => p.id === pageId)) pageId = view.pages[0]?.id ?? HOME_PAGE;
    drawPage(ctx, view.objects, pageId, images, schedule);
    if (draft) drawObject(ctx, draft, images, schedule);
    publishStats();
    syncChrome();
  }

  function syncChrome(): void {
    for (const [t, b] of buttons) b.setAttribute('aria-checked', String(t === tool));
    for (const b of swatches) b.setAttribute('aria-checked', String(b.dataset.color === color));
    const view = board.view();
    delPageBtn.disabled = view.pages.length <= 1;
    undoBtn.disabled = !canUndo;
    redoBtn.disabled = !canRedo;
    statusEl.textContent = [status, S.status(board.liveCount(), board.compactions)]
      .filter(Boolean)
      .join('  ');
    if (!editingPage) renderPages();
  }

  /** Numbers on the canvas element, for tests and for the live spec. */
  function publishStats(): void {
    const view = board.view();
    canvas.dataset.objects = String(view.objects.filter((o) => o.pg === pageId).length);
    canvas.dataset.allObjects = String(view.objects.length);
    canvas.dataset.pages = String(view.pages.length);
    canvas.dataset.compactions = String(board.compactions);
    canvas.dataset.live = String(board.liveCount());
  }

  function onBoardChange(): void {
    publishStats();
    const all = board.allEntries();
    canUndo = undoTarget(all, board.me) !== null;
    canRedo = redoTarget(all, board.me) !== null;
    schedule();
  }

  // --- pointer ---------------------------------------------------------------------------------------------
  canvas.addEventListener('pointerdown', (ev) => {
    if (ev.button !== 0 && ev.pointerType === 'mouse') return;
    const p = toPage(ev);
    commitText();
    if (tool === 'text') {
      beginText(p, ev);
      ev.preventDefault();
      return;
    }
    if (tool === 'image') {
      pendingImageAt = rounded(p);
      status = '';
      urlInput.focus();
      return;
    }
    canvas.setPointerCapture(ev.pointerId);
    start = p;
    if (tool === 'eraser') {
      erase(p);
      return;
    }
    raw = [p];
    updateDraft(p);
  });

  canvas.addEventListener('pointermove', (ev) => {
    if (!start) return;
    const p = toPage(ev);
    if (tool === 'eraser') {
      erase(p);
      return;
    }
    if (tool === 'pen' || tool === 'highlighter') raw.push(p);
    updateDraft(p);
  });

  const finish = (ev: PointerEvent): void => {
    if (!start) return;
    const p = toPage(ev);
    const s = start;
    start = null;
    if (tool === 'eraser') return;
    if (tool === 'pen' || tool === 'highlighter') {
      raw.push(p);
      const pts = packStroke(raw);
      if (pts.length >= 2) {
        board.add({
          k: 'stroke',
          pg: pageId,
          c: color,
          w: widthFor(tool),
          hl: tool === 'highlighter',
          pts,
        });
      }
    } else if (tool === 'line' || tool === 'arrow' || tool === 'rect' || tool === 'ellipse') {
      if (Math.hypot(p[0] - s[0], p[1] - s[1]) >= 4) {
        board.add({
          k: 'shape',
          pg: pageId,
          s: tool as ShapeKind,
          c: color,
          w: width(),
          fill: fillEl.checked && (tool === 'rect' || tool === 'ellipse'),
          a: rounded(s),
          b: rounded(p),
        });
      }
    }
    draft = null;
    raw = [];
    schedule();
  };
  canvas.addEventListener('pointerup', finish);
  canvas.addEventListener('pointercancel', () => {
    start = null;
    draft = null;
    raw = [];
    schedule();
  });

  function updateDraft(p: Pt): void {
    if (!start) return;
    const base = { id: 'draft', author: 'me', pg: pageId, c: color };
    if (tool === 'pen' || tool === 'highlighter') {
      draft = {
        ...base,
        k: 'stroke',
        w: widthFor(tool),
        hl: tool === 'highlighter',
        pts: raw.flatMap((q) => [Math.round(q[0]), Math.round(q[1])]).slice(-8000),
      } as DrawObject;
    } else if (tool === 'line' || tool === 'arrow' || tool === 'rect' || tool === 'ellipse') {
      draft = {
        ...base,
        k: 'shape',
        s: tool,
        w: width(),
        fill: fillEl.checked && (tool === 'rect' || tool === 'ellipse'),
        a: rounded(start),
        b: rounded(p),
      } as DrawObject;
    }
    schedule();
  }

  function erase(p: Pt): void {
    const view = board.view();
    const o = pick(view.objects, pageId, p, Math.max(6, width()));
    if (o) board.add({ k: 'erase', t: o.id });
  }

  // --- text ------------------------------------------------------------------------------------------------
  function beginText(p: Pt, ev: PointerEvent): void {
    pendingText = rounded(p);
    const stage = canvas.parentElement as HTMLElement;
    const sr = stage.getBoundingClientRect();
    textEl.style.insetInlineStart = `${ev.clientX - sr.left}px`;
    textEl.style.insetBlockStart = `${ev.clientY - sr.top}px`;
    const px = Math.max(
      10,
      Math.round(textSize() * (canvas.getBoundingClientRect().width / PAGE_W)),
    );
    textEl.style.fontSize = `${px}px`;
    textEl.style.color = color;
    textEl.value = '';
    textEl.hidden = false;
    setTimeout(() => textEl.focus(), 0);
  }
  const textSize = (): number => 16 + width() * 3;

  function commitText(): void {
    if (textEl.hidden || !pendingText) {
      textEl.hidden = true;
      return;
    }
    const text = textEl.value.replace(/\s+$/, '').slice(0, 500);
    const at = pendingText;
    pendingText = null;
    textEl.hidden = true;
    textEl.value = '';
    if (text.trim() !== '')
      board.add({ k: 'text', pg: pageId, c: color, size: textSize(), at, text });
  }
  textEl.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      commitText();
    } else if (ev.key === 'Escape') {
      pendingText = null;
      textEl.hidden = true;
    }
    ev.stopPropagation();
  });
  textEl.addEventListener('blur', commitText);

  // --- images ----------------------------------------------------------------------------------------------
  function placeImage(img: ImageResult): void {
    const { w, h } = fitImage(img.w, img.h, PAGE_W, PAGE_H);
    const at = pendingImageAt ?? ([PAGE_W / 2, PAGE_H / 2] as Pt);
    const x = Math.round(Math.min(Math.max(0, at[0] - w / 2), PAGE_W - w));
    const y = Math.round(Math.min(Math.max(0, at[1] - h / 2), PAGE_H - h));
    board.add({ k: 'image', pg: pageId, src: img.src, at: [x, y], size: [w, h] });
    pendingImageAt = null;
    urlInput.value = '';
  }
  const imageError = (e: unknown): void => {
    const m = e instanceof Error ? e.message : '';
    status = m === 'too-big' ? S.imageTooBig : m === 'https' ? S.imageHttps : S.imageBad;
    syncChrome();
  };
  urlAdd.addEventListener('click', () => {
    const v = urlInput.value.trim();
    if (!v) return fileEl.click();
    urlToImage(v).then(placeImage, imageError);
  });
  urlInput.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') urlAdd.click();
    ev.stopPropagation();
  });
  fileBtn.addEventListener('click', () => fileEl.click());
  fileEl.addEventListener('change', () => {
    const f = fileEl.files?.[0];
    fileEl.value = '';
    if (f) fileToImage(f).then(placeImage, imageError);
  });

  // --- toolbar and pages -----------------------------------------------------------------------------------
  undoBtn.addEventListener('click', () => void board.undo());
  redoBtn.addEventListener('click', () => void board.redo());

  const armed = new WeakMap<
    HTMLButtonElement,
    { timer: ReturnType<typeof setTimeout>; label: string }
  >();
  /** Two-step confirm without a modal: the first click changes the label, the second does the thing. */
  function confirmed(btn: HTMLButtonElement, question: string, run: () => void): void {
    const a = armed.get(btn);
    if (a) {
      clearTimeout(a.timer);
      btn.textContent = a.label;
      armed.delete(btn);
      run();
      return;
    }
    const label = btn.textContent ?? '';
    btn.textContent = question;
    armed.set(btn, {
      label,
      timer: setTimeout(() => {
        btn.textContent = label;
        armed.delete(btn);
      }, 4000),
    });
  }
  clearBtn.addEventListener('click', () =>
    confirmed(clearBtn, S.clearConfirm, () => board.add({ k: 'clear', pg: pageId })),
  );
  delPageBtn.addEventListener('click', () =>
    confirmed(delPageBtn, S.pageDeleteConfirm, () => {
      const pages = board.view().pages;
      if (pages.length <= 1) {
        status = S.pageLast;
        return syncChrome();
      }
      board.add({ k: 'pagedel', t: pageId });
    }),
  );
  addPageBtn.addEventListener('click', () => {
    const n = board.view().pages.length + 1;
    pageId = board.add({ k: 'page', name: `Page ${n}` });
    schedule();
  });
  renameBtn.addEventListener('click', () => {
    const current = board.view().pages.find((p) => p.id === pageId);
    editingPage = true;
    const input = el('input', { type: 'text', value: current?.name ?? '', maxLength: 40 });
    input.setAttribute('aria-label', S.pageRenamePrompt);
    pagesEl.replaceChildren(input);
    const done = (save: boolean): void => {
      if (!editingPage) return;
      editingPage = false;
      const name = input.value.trim().slice(0, 40);
      if (save && name && name !== current?.name) board.add({ k: 'rename', t: pageId, name });
      syncChrome();
    };
    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') done(true);
      else if (ev.key === 'Escape') done(false);
      ev.stopPropagation();
    });
    input.addEventListener('blur', () => done(true));
    input.focus();
    input.select();
  });
  function renderPages(): void {
    const pages = board.view().pages;
    pagesEl.replaceChildren(
      ...pages.map((p) => {
        const b = el('button', { type: 'button', textContent: p.name });
        b.setAttribute('role', 'tab');
        b.setAttribute('aria-selected', String(p.id === pageId));
        b.addEventListener('click', () => {
          commitText();
          pageId = p.id;
          schedule();
        });
        return b;
      }),
    );
  }

  exportBtn.addEventListener('click', () => {
    const view = board.view();
    const name = view.pages.find((p) => p.id === pageId)?.name ?? 'page';
    const file = `whiteboard-${name.replace(/[^\w-]+/g, '-').toLowerCase() || 'page'}.png`;
    renderPagePng(view.objects, pageId, images).then(
      (blob) => deliverPng(blob, file),
      () => {
        status = S.imageBad;
        syncChrome();
      },
    );
  });

  const onKey = (ev: KeyboardEvent): void => {
    const mod = ev.ctrlKey || ev.metaKey;
    if (mod && ev.key.toLowerCase() === 'z') {
      ev.preventDefault();
      if (ev.shiftKey) board.redo();
      else board.undo();
    } else if (mod && ev.key.toLowerCase() === 'y') {
      ev.preventDefault();
      board.redo();
    }
  };
  document.addEventListener('keydown', onKey);

  const unsub = board.subscribe(onBoardChange);
  onBoardChange();

  return () => {
    unsub();
    document.removeEventListener('keydown', onKey);
    if (raf) cancelAnimationFrame(raf);
  };
}
