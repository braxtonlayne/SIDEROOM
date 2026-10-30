import { Icon } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent } from 'react';
import { useT } from '../../i18n';
import { getTool, type ToolEntry } from '../registry';
import { ToolView } from '../ToolView';
import { clampRect, type Gadget, type GadgetRect } from './layout';
import { useToolsStore, viewport } from './store';

const STEP = 16;
const isRtl = () => typeof document !== 'undefined' && getComputedStyle(document.documentElement).direction === 'rtl';

/**
 * UTL-SHL-001: the floating tools. Rendered by the drawer host, which the router keeps mounted across routes, so a
 * gadget survives navigation exactly like the player. Positions are measured from the inline-start / block-start
 * corner (they mirror in right-to-left). A gadget is a non-modal dialog: no focus trap, and Escape (which closes the
 * drawer) does nothing here.
 */
export function GadgetLayer() {
  const gadgets = useToolsStore((s) => s.layout.gadgets);
  // A smaller window must not leave a gadget off screen.
  useEffect(() => {
    const onResize = () => {
      const { layout, setRect } = useToolsStore.getState();
      for (const g of layout.gadgets) setRect(g.tool, g);
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return (
    <div className="sr-tl-gadgets">
      {gadgets.map((g) => {
        const entry = getTool(g.tool);
        return entry ? <GadgetWindow key={g.tool} gadget={g} entry={entry} /> : null;
      })}
    </div>
  );
}

type Gesture = {
  kind: 'move' | 'size';
  startX: number;
  startY: number;
  from: GadgetRect;
  sign: 1 | -1;
};

function GadgetWindow({ gadget, entry }: { gadget: Gadget; entry: ToolEntry }) {
  const t = useT();
  const gesture = useRef<Gesture | null>(null);
  const [live, setLive] = useState<GadgetRect | null>(null);
  const pinned = useToolsStore((s) => s.layout.pins.includes(entry.id));
  const rect = live ?? gadget;
  const mins = entry.size;

  const begin = (kind: Gesture['kind']) => (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('button')) return;
    useToolsStore.getState().raise(entry.id);
    gesture.current = {
      kind,
      startX: e.clientX,
      startY: e.clientY,
      from: { x: gadget.x, y: gadget.y, w: gadget.w, h: gadget.h },
      sign: isRtl() ? -1 : 1,
    };
    e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  };
  const next = (e: ReactPointerEvent<HTMLElement>): GadgetRect | null => {
    const g = gesture.current;
    if (!g) return null;
    const dx = (e.clientX - g.startX) * g.sign;
    const dy = e.clientY - g.startY;
    const raw =
      g.kind === 'move'
        ? { ...g.from, x: g.from.x + dx, y: g.from.y + dy }
        : { ...g.from, w: g.from.w + dx, h: g.from.h + dy };
    return clampRect(raw, viewport(), mins);
  };
  const move = (e: ReactPointerEvent<HTMLElement>) => {
    const r = next(e);
    if (r) setLive(r);
  };
  const end = (e: ReactPointerEvent<HTMLElement>) => {
    const r = next(e);
    gesture.current = null;
    setLive(null);
    if (r) useToolsStore.getState().setRect(entry.id, r);
  };

  const onKey = (e: ReactKeyboardEvent<HTMLElement>) => {
    if (e.target !== e.currentTarget || e.ctrlKey || e.metaKey || e.altKey) return;
    const dir = isRtl() ? -1 : 1;
    let dx = 0;
    let dy = 0;
    if (e.key === 'ArrowLeft') dx = -STEP * dir;
    else if (e.key === 'ArrowRight') dx = STEP * dir;
    else if (e.key === 'ArrowUp') dy = -STEP;
    else if (e.key === 'ArrowDown') dy = STEP;
    else return;
    e.preventDefault();
    const base = { x: gadget.x, y: gadget.y, w: gadget.w, h: gadget.h };
    useToolsStore
      .getState()
      .setRect(
        entry.id,
        e.shiftKey ? { ...base, w: base.w + dx, h: base.h + dy } : { ...base, x: base.x + dx, y: base.y + dy },
      );
  };

  const name = t(entry.nameKey);
  return (
    // The dialog is non-modal: a pointer press anywhere in it brings it to the front.
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
    <section
      role="dialog"
      aria-modal="false"
      aria-label={name}
      className="sr-tl-gadget"
      data-tool={entry.id}
      style={{
        insetInlineStart: rect.x,
        insetBlockStart: rect.y,
        inlineSize: rect.w,
        blockSize: rect.h,
        zIndex: gadget.z,
      }}
      onPointerDownCapture={() => useToolsStore.getState().raise(entry.id)}
    >
      <div
        className="sr-tl-gadget-head"
        role="toolbar"
        tabIndex={0}
        aria-label={t('tools.gadget.move', { name })}
        onPointerDown={begin('move')}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
        onKeyDown={onKey}
      >
        <Icon name={entry.icon} size={16} />
        <span className="sr-tl-gadget-title">{name}</span>
        {entry.floatable !== false && (
          <button
            type="button"
            className="sr-tl-icon-btn"
            aria-pressed={pinned}
            aria-label={pinned ? t('tools.drawer.unpin') : t('tools.drawer.pin')}
            onClick={() =>
              pinned ? useToolsStore.getState().unpin(entry.id) : useToolsStore.getState().pin(entry.id)
            }
          >
            <Icon name="pin" size={16} />
          </button>
        )}
        <button
          type="button"
          className="sr-tl-icon-btn"
          aria-label={t('tools.gadget.close', { name })}
          onClick={() => useToolsStore.getState().close(entry.id)}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
      <div className="sr-tl-gadget-body">
        <ToolView
          entry={entry}
          host="gadget"
          onClose={() => useToolsStore.getState().close(entry.id)}
        />
      </div>
      <div
        className="sr-tl-gadget-grip"
        aria-hidden="true"
        onPointerDown={begin('size')}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
      />
    </section>
  );
}
