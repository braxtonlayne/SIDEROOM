// The editor's grid (EXT-PRF-005): blocks are tiles on a 12 column grid. Drag a tile to move it, drag its
// corner to resize it. The same works from the keyboard: arrow keys move the selected tile, Shift with an
// arrow resizes it, Delete removes it. A move or resize that would overlap another tile or leave the grid is
// refused, the tile stays where it was.
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { useT, type MessageKey } from '../i18n';
import { moveBlock, resizeBlock, rowsFor } from './grid';
import { GRID_COLUMNS, type Block, type BlockKind } from './types';

export const ROW_PX = 40;

export const KIND_LABEL: Record<BlockKind, MessageKey> = {
  header: 'lounge.profileEditor.kindHeader',
  about: 'lounge.profileEditor.kindAbout',
  top_friends: 'lounge.profileEditor.kindFriends',
  now_watching: 'lounge.profileEditor.kindNow',
  favorites: 'lounge.profileEditor.kindFavorites',
  gallery: 'lounge.profileEditor.kindGallery',
  guestbook: 'lounge.profileEditor.kindGuestbook',
  song: 'lounge.profileEditor.kindSong',
  custom: 'lounge.profileEditor.kindCustom',
  linktree: 'profile.editor.kindLinks',
  featured_guilds: 'profile.editor.kindServers',
  now_playing: 'profile.editor.kindPlaying',
};

interface Drag {
  id: string;
  mode: 'move' | 'resize';
  startX: number;
  startY: number;
  base: Block;
}

export function Canvas({
  blocks,
  selected,
  onSelect,
  onChange,
  onRemove,
}: {
  blocks: Block[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  onChange: (next: Block[]) => void;
  onRemove: (id: string) => void;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [cellW, setCellW] = useState(60);
  const [drag, setDrag] = useState<Drag | null>(null);
  const rows = rowsFor(blocks);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const w = el.getBoundingClientRect().width;
      if (w > 0) setCellW(w / GRID_COLUMNS);
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const down = (e: PointerEvent<HTMLElement>, b: Block, mode: Drag['mode']) => {
    onSelect(b.id);
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDrag({ id: b.id, mode, startX: e.clientX, startY: e.clientY, base: b });
    e.stopPropagation();
  };
  const move = (e: PointerEvent<HTMLElement>) => {
    if (!drag) return;
    const dx = Math.round((e.clientX - drag.startX) / cellW);
    const dy = Math.round((e.clientY - drag.startY) / ROW_PX);
    const b = drag.base;
    // Always start from the block as it was grabbed, so a refused step does not stick.
    const others = blocks.map((k) => (k.id === b.id ? b : k));
    const next =
      drag.mode === 'move'
        ? moveBlock(others, b.id, b.x + dx, b.y + dy)
        : resizeBlock(others, b.id, b.w + dx, b.h + dy);
    onChange(next);
  };
  const up = () => setDrag(null);

  const key = (e: KeyboardEvent<HTMLElement>, b: Block) => {
    const step: Record<string, [number, number]> = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      onRemove(b.id);
      return;
    }
    const s = step[e.key];
    if (!s) return;
    e.preventDefault();
    onChange(
      e.shiftKey
        ? resizeBlock(blocks, b.id, b.w + s[0], b.h + s[1])
        : moveBlock(blocks, b.id, b.x + s[0], b.y + s[1]),
    );
  };

  return (
    <div
      ref={ref}
      className="pp-canvas"
      role="group"
      aria-label={t('lounge.profileEditor.canvasLabel')}
      style={{
        gridTemplateRows: `repeat(${rows}, ${ROW_PX}px)`,
        backgroundSize: `calc(100% / ${GRID_COLUMNS}) ${ROW_PX}px`,
      }}
      onClick={() => onSelect(null)}
    >
      {blocks.map((b) => (
        <div
          key={b.id}
          role="button"
          tabIndex={0}
          aria-pressed={selected === b.id}
          data-dragging={drag?.id === b.id || undefined}
          aria-label={t('lounge.profileEditor.tileLabel', {
            kind: t(KIND_LABEL[b.type]),
            x: b.x + 1,
            y: b.y + 1,
            w: b.w,
            h: b.h,
          })}
          className="pp-tile"
          style={{ gridColumn: `${b.x + 1} / span ${b.w}`, gridRow: `${b.y + 1} / span ${b.h}` }}
          onClick={(e) => {
            e.stopPropagation();
            onSelect(b.id);
          }}
          onPointerDown={(e) => down(e, b, 'move')}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={up}
          onKeyDown={(e) => key(e, b)}
          onFocus={() => onSelect(b.id)}
        >
          <strong>{t(KIND_LABEL[b.type])}</strong>
          <span>{`${b.w} × ${b.h}`}</span>
          <span
            className="pp-grip"
            aria-hidden="true"
            data-testid={`grip-${b.id}`}
            onPointerDown={(e) => down(e, b, 'resize')}
            onPointerMove={move}
            onPointerUp={up}
            onPointerCancel={up}
          />
        </div>
      ))}
    </div>
  );
}
