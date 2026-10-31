/**
 * UTL-IMG-001: the image editor. One working bitmap in a {@link StudioSession}; the canvas shows it, a pointer
 * drag selects a region for crop and redaction, and a panel per mode holds the options. Nothing in this file
 * (or the session) sends the image anywhere: "Use the edited image" hands a Blob to the caller, which decides.
 */
import { Button, Icon } from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import { announce } from '../../a11y/announcer';
import { useT } from '../../i18n';
import { clampRegion, type Bitmap, type Region } from './bitmap';
import { DEFAULT_CAPTION, type CaptionFont, type CaptionOptions } from './caption';
import { FORMAT_EXT, FORMAT_MIME, paintBitmap, type ExportFormat } from './canvasIo';
import { StudioSession } from './session';
import { loadStudioWasm } from './wasm';
import './studio.css';

export type StudioMode = 'crop' | 'resize' | 'pad' | 'caption' | 'redact' | 'palette' | 'export';
const MODES: readonly StudioMode[] = [
  'crop',
  'resize',
  'pad',
  'caption',
  'redact',
  'palette',
  'export',
];

export interface StudioEditorProps {
  bitmap: Bitmap;
  /** The name the exported file starts from (without the extension). */
  baseName?: string;
  initialMode?: StudioMode;
  /** Only these modes are offered (the redact and palette tools narrow the studio to one job). */
  modes?: readonly StudioMode[];
  /** Hand the edited image back (the composer replaces the attachment with it). */
  onUse?: (file: File) => void;
  useLabel?: 'edited' | 'copy';
}

interface Swatch {
  hex: string;
  share: number;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function StudioEditor({
  bitmap,
  baseName = 'image',
  initialMode = 'crop',
  modes = MODES,
  onUse,
  useLabel = 'edited',
}: StudioEditorProps) {
  const t = useT();
  const session = useMemo(() => new StudioSession(bitmap), [bitmap]);
  const [version, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const [mode, setMode] = useState<StudioMode>(
    modes.includes(initialMode) ? initialMode : modes[0]!,
  );
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const cur = session.current;

  // The selection, in CSS pixels inside the canvas box.
  const [drag, setDrag] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const dragging = useRef(false);
  const [sel, setSel] = useState<Region | null>(null);

  useEffect(() => {
    if (canvasRef.current) paintBitmap(canvasRef.current, cur);
    setSel(null);
    setDrag(null);
  }, [cur, version]);

  const point = (e: PointerEvent): { x: number; y: number } => {
    const r = canvasRef.current!.getBoundingClientRect();
    return {
      x: Math.min(Math.max(0, e.clientX - r.left), r.width),
      y: Math.min(Math.max(0, e.clientY - r.top), r.height),
    };
  };
  const onDown = (e: PointerEvent) => {
    if (mode !== 'crop' && mode !== 'redact') return;
    dragging.current = true;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    const p = point(e);
    setDrag({ x0: p.x, y0: p.y, x1: p.x, y1: p.y });
    setSel(null);
  };
  const onMove = (e: PointerEvent) => {
    if (!dragging.current) return;
    const p = point(e);
    setDrag((d) => (d ? { ...d, x1: p.x, y1: p.y } : d));
  };
  const onUp = () => {
    if (!dragging.current) return;
    dragging.current = false;
    setDrag((d) => {
      if (d && canvasRef.current) {
        const box = canvasRef.current.getBoundingClientRect();
        const k = cur.width / (box.width || cur.width);
        const r = clampRegion(cur, {
          x: Math.min(d.x0, d.x1) * k,
          y: Math.min(d.y0, d.y1) * k,
          width: Math.abs(d.x1 - d.x0) * k,
          height: Math.abs(d.y1 - d.y0) * k,
        });
        setSel(r && r.width > 2 && r.height > 2 ? r : null);
      }
      return d;
    });
  };
  const box = drag
    ? {
        insetInlineStart: Math.min(drag.x0, drag.x1),
        insetBlockStart: Math.min(drag.y0, drag.y1),
        inlineSize: Math.abs(drag.x1 - drag.x0),
        blockSize: Math.abs(drag.y1 - drag.y0),
      }
    : null;

  // --- mode state ---
  const [w, setW] = useState(cur.width);
  const [h, setH] = useState(cur.height);
  const [lock, setLock] = useState(true);
  useEffect(() => {
    setW(cur.width);
    setH(cur.height);
  }, [cur.width, cur.height, version]);
  const [border, setBorder] = useState(32);
  const [borderColour, setBorderColour] = useState('#ffffff');
  const [caption, setCaption] = useState<CaptionOptions>(DEFAULT_CAPTION);
  const [block, setBlock] = useState(12);
  const [count, setCount] = useState(6);
  const [swatches, setSwatches] = useState<Swatch[] | null>(null);
  const [copied, setCopied] = useState('');
  const [format, setFormat] = useState<ExportFormat>('png');
  const [quality, setQuality] = useState(0.92);
  const [name, setName] = useState(baseName);
  const [size, setSize] = useState<number | null>(null);
  const [notice, setNotice] = useState('');

  const hexToRgba = (hex: string): [number, number, number, number] => {
    const n = Number.parseInt(hex.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 255];
  };
  const edit = (fn: () => void, message?: string) => {
    fn();
    setSwatches(null);
    setSize(null);
    if (message) {
      setNotice(message);
      announce(message);
    }
    bump();
  };

  // The size of the file an export would make (re-encoded when the format or quality changes).
  useEffect(() => {
    if (mode !== 'export') return;
    let live = true;
    session
      .exportAs(format, quality)
      .then((b) => live && setSize(b.size))
      .catch(() => live && setSize(null));
    return () => {
      live = false;
    };
  }, [mode, format, quality, session, version]);

  const encode = async (): Promise<File> => {
    const blob = await session.exportAs(format, quality);
    const ext = blob.type === FORMAT_MIME[format] ? FORMAT_EXT[format] : 'png';
    if (format === 'webp' && ext === 'png') setNotice(t('studio.export.webpFallback'));
    return new File([blob], `${name.trim() || baseName}.${ext}`, { type: blob.type });
  };

  const download = async () => {
    const file = await encode();
    const url = URL.createObjectURL(file);
    const a = document.createElement('a');
    a.href = url;
    a.download = file.name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 2000);
  };

  const extract = async () => {
    const tools = await loadStudioWasm();
    const list = tools.palette.extract(
      new Uint8Array(cur.data.buffer, cur.data.byteOffset, cur.data.byteLength),
      count,
    );
    setSwatches(list.map((c) => ({ hex: c.hex, share: c.share })));
    if (list.length === 0) announce(t('studio.palette.empty'));
  };

  const copy = (hex: string) => {
    void navigator.clipboard?.writeText(hex);
    setCopied(hex);
    announce(t('studio.palette.copied', { hex }));
  };

  const modeLabel: Record<StudioMode, string> = {
    crop: t('studio.mode.crop'),
    resize: t('studio.mode.resize'),
    pad: t('studio.mode.pad'),
    caption: t('studio.mode.caption'),
    redact: t('studio.mode.redact'),
    palette: t('studio.mode.palette'),
    export: t('studio.mode.export'),
  };
  const fontLabel: Record<CaptionFont, string> = {
    display: t('studio.caption.fontDisplay'),
    body: t('studio.caption.fontBody'),
    mono: t('studio.caption.fontMono'),
  };

  const setWidth = (v: number) => {
    setW(v);
    if (lock) setH(Math.max(1, Math.round((v * cur.height) / cur.width)));
  };
  const setHeight = (v: number) => {
    setH(v);
    if (lock) setW(Math.max(1, Math.round((v * cur.width) / cur.height)));
  };

  return (
    <div className="sr-st">
      <p className="sr-tl-muted sr-st-privacy">{t('studio.privacy.note')}</p>

      <div className="sr-st-bar" role="toolbar" aria-label={t('studio.modes')}>
        {modes.map((m) => (
          <Button
            key={m}
            size="sm"
            variant={m === mode ? 'primary' : 'ghost'}
            aria-pressed={m === mode}
            onClick={() => setMode(m)}
          >
            {modeLabel[m]}
          </Button>
        ))}
        <span className="sr-st-spacer" />
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={t('studio.undo')}
          disabled={!session.canUndo}
          onClick={() => edit(() => session.undo())}
        >
          <Icon name="arrow-left" size={16} />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          iconOnly
          aria-label={t('studio.redo')}
          disabled={!session.canRedo}
          onClick={() => edit(() => session.redo())}
        >
          <Icon name="arrow-right" size={16} />
        </Button>
      </div>

      <div
        className="sr-st-stage"
        data-selecting={mode === 'crop' || mode === 'redact' ? '' : undefined}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
      >
        <canvas
          ref={canvasRef}
          className="sr-st-canvas"
          role="img"
          aria-label={t('studio.canvas.label', { width: cur.width, height: cur.height })}
        />
        {box && <div className="sr-st-sel" style={box} />}
      </div>
      {(mode === 'crop' || mode === 'redact') && (
        <p className="sr-tl-muted" role="status">
          {sel
            ? t('studio.canvas.selection', { width: sel.width, height: sel.height })
            : t('studio.canvas.noSelection')}
        </p>
      )}

      <div className="sr-st-panel">
        {mode === 'crop' && (
          <>
            <Button size="sm" disabled={!sel} onClick={() => sel && edit(() => session.crop(sel))}>
              {t('studio.crop.apply')}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => edit(() => session.rotate(false))}>
              {t('studio.rotateLeft')}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => edit(() => session.rotate(true))}>
              {t('studio.rotateRight')}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => edit(() => session.flip(true))}>
              {t('studio.flipHorizontal')}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => edit(() => session.flip(false))}>
              {t('studio.flipVertical')}
            </Button>
          </>
        )}

        {mode === 'resize' && (
          <>
            <label className="sr-st-field">
              <span>{t('studio.resize.width')}</span>
              <input
                type="number"
                min={1}
                max={4096}
                value={w}
                onChange={(e) => setWidth(Number(e.target.value) || 1)}
              />
            </label>
            <label className="sr-st-field">
              <span>{t('studio.resize.height')}</span>
              <input
                type="number"
                min={1}
                max={4096}
                value={h}
                onChange={(e) => setHeight(Number(e.target.value) || 1)}
              />
            </label>
            <label className="sr-st-check">
              <input type="checkbox" checked={lock} onChange={(e) => setLock(e.target.checked)} />
              <span>{t('studio.resize.lock')}</span>
            </label>
            <Button
              size="sm"
              onClick={() => edit(() => session.resize(Math.min(4096, w), Math.min(4096, h)))}
            >
              {t('studio.resize.apply')}
            </Button>
            <div className="sr-st-row" role="group" aria-label={t('studio.resize.presets')}>
              {[256, 512, 1024, 1920].map((edge) => (
                <Button
                  key={edge}
                  size="sm"
                  variant="secondary"
                  disabled={Math.max(cur.width, cur.height) <= edge}
                  onClick={() => {
                    const k = edge / Math.max(cur.width, cur.height);
                    edit(() => session.resize(cur.width * k, cur.height * k));
                  }}
                >
                  {edge}
                </Button>
              ))}
            </div>
          </>
        )}

        {mode === 'pad' && (
          <>
            <label className="sr-st-field">
              <span>{t('studio.pad.amount')}</span>
              <input
                type="number"
                min={0}
                max={2048}
                value={border}
                onChange={(e) => setBorder(Math.max(0, Number(e.target.value) || 0))}
              />
            </label>
            <label className="sr-st-field">
              <span>{t('studio.pad.colour')}</span>
              <input
                type="color"
                value={borderColour}
                onChange={(e) => setBorderColour(e.target.value)}
              />
            </label>
            <Button
              size="sm"
              onClick={() =>
                edit(() =>
                  session.pad(
                    { top: border, right: border, bottom: border, left: border },
                    hexToRgba(borderColour),
                  ),
                )
              }
            >
              {t('studio.pad.apply')}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => edit(() => session.padToAspect(1, hexToRgba(borderColour)))}
            >
              {t('studio.pad.square')}
            </Button>
          </>
        )}

        {mode === 'caption' && (
          <>
            <label className="sr-st-field sr-st-wide">
              <span>{t('studio.caption.top')}</span>
              <input
                type="text"
                value={caption.top}
                onChange={(e) => setCaption({ ...caption, top: e.target.value })}
              />
            </label>
            <label className="sr-st-field sr-st-wide">
              <span>{t('studio.caption.bottom')}</span>
              <input
                type="text"
                value={caption.bottom}
                onChange={(e) => setCaption({ ...caption, bottom: e.target.value })}
              />
            </label>
            <label className="sr-st-field">
              <span>{t('studio.caption.font')}</span>
              <select
                value={caption.font}
                onChange={(e) => setCaption({ ...caption, font: e.target.value as CaptionFont })}
              >
                {(Object.keys(fontLabel) as CaptionFont[]).map((f) => (
                  <option key={f} value={f}>
                    {fontLabel[f]}
                  </option>
                ))}
              </select>
            </label>
            <label className="sr-st-field">
              <span>{t('studio.caption.size')}</span>
              <input
                type="range"
                min={0.5}
                max={2}
                step={0.1}
                value={caption.size}
                onChange={(e) => setCaption({ ...caption, size: Number(e.target.value) })}
              />
            </label>
            <label className="sr-st-check">
              <input
                type="checkbox"
                checked={caption.uppercase}
                onChange={(e) => setCaption({ ...caption, uppercase: e.target.checked })}
              />
              <span>{t('studio.caption.uppercase')}</span>
            </label>
            <Button
              size="sm"
              disabled={!caption.top.trim() && !caption.bottom.trim()}
              onClick={() => {
                edit(() => session.caption(caption));
                setCaption({ ...caption, top: '', bottom: '' });
              }}
            >
              {t('studio.caption.apply')}
            </Button>
          </>
        )}

        {mode === 'redact' && (
          <>
            <Button
              size="sm"
              disabled={!sel}
              onClick={() => sel && edit(() => session.redact(sel), t('studio.redact.done'))}
            >
              {t('studio.redact.cover')}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={!sel}
              onClick={() =>
                sel && edit(() => session.pixelate(sel, block), t('studio.redact.donePixelated'))
              }
            >
              {t('studio.redact.pixelate')}
            </Button>
            <label className="sr-st-field">
              <span>{t('studio.redact.block')}</span>
              <input
                type="range"
                min={4}
                max={64}
                value={block}
                onChange={(e) => setBlock(Number(e.target.value))}
              />
            </label>
            <Button
              size="sm"
              variant="ghost"
              disabled={!session.canUndo && !session.canRedo}
              onClick={() => {
                session.forgetHistory();
                setNotice(t('studio.redact.forgotten'));
                announce(t('studio.redact.forgotten'));
                bump();
              }}
            >
              {t('studio.redact.forget')}
            </Button>
            <p className="sr-tl-muted sr-st-wide">{t('studio.redact.note')}</p>
          </>
        )}

        {mode === 'palette' && (
          <>
            <label className="sr-st-field">
              <span>{t('studio.palette.count')}</span>
              <input
                type="number"
                min={2}
                max={16}
                value={count}
                onChange={(e) => setCount(Math.min(16, Math.max(2, Number(e.target.value) || 6)))}
              />
            </label>
            <Button size="sm" onClick={() => void extract()}>
              {t('studio.palette.extract')}
            </Button>
            {swatches && (
              <ul className="sr-st-swatches" aria-label={t('studio.palette.list')}>
                {swatches.map((s) => (
                  <li key={s.hex}>
                    <button
                      type="button"
                      className="sr-st-swatch"
                      style={{ background: s.hex }}
                      aria-label={t('studio.palette.copy', { hex: s.hex })}
                      title={t('studio.palette.share', {
                        hex: s.hex,
                        percent: Math.round(s.share * 100),
                      })}
                      onClick={() => copy(s.hex)}
                    />
                    <code>
                      {copied === s.hex ? t('studio.palette.copied', { hex: s.hex }) : s.hex}
                    </code>
                  </li>
                ))}
              </ul>
            )}
            {swatches && swatches.length === 0 && (
              <p className="sr-tl-muted">{t('studio.palette.empty')}</p>
            )}
          </>
        )}

        {mode === 'export' && (
          <>
            <label className="sr-st-field">
              <span>{t('studio.export.format')}</span>
              <select value={format} onChange={(e) => setFormat(e.target.value as ExportFormat)}>
                <option value="png">{t('studio.export.formatPng')}</option>
                <option value="jpeg">{t('studio.export.formatJpeg')}</option>
                <option value="webp">{t('studio.export.formatWebp')}</option>
              </select>
            </label>
            {format !== 'png' && (
              <label className="sr-st-field">
                <span>{t('studio.export.quality')}</span>
                <input
                  type="range"
                  min={0.3}
                  max={1}
                  step={0.01}
                  value={quality}
                  onChange={(e) => setQuality(Number(e.target.value))}
                />
              </label>
            )}
            <label className="sr-st-field">
              <span>{t('studio.export.name')}</span>
              <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            {size !== null && (
              <p className="sr-tl-muted sr-st-wide">
                {t('studio.export.size', { size: fmtBytes(size) })}
              </p>
            )}
            <Button size="sm" variant="secondary" onClick={() => void download()}>
              {t('studio.export.download')}
            </Button>
            {onUse && (
              <Button size="sm" onClick={() => void encode().then(onUse)}>
                {useLabel === 'copy' ? t('studio.use.copy') : t('studio.use.edited')}
              </Button>
            )}
          </>
        )}
      </div>
      <p className="sr-tl-muted" role="status">
        {notice}
      </p>
    </div>
  );
}
