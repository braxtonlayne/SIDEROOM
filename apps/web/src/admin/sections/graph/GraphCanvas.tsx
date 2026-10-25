// Canvas 2D renderer of the invite graph (ADM-GRAPH-003; no WebGL, owner decision P10-ADM-8 / G-24). Used when the
// graph passes about 500 nodes. The layout comes from `sr-graph-wasm`; this draws it: pan (drag), zoom (wheel,
// buttons, + and -), click to select, double click or Enter to collapse and expand a subtree, colour by account
// state, a halo for suspicion and a cross for a banned account. The tree and list views stay as the accessible
// alternative to the picture. Reduced motion (the OS setting, or Settings > Accessibility): the picture is drawn
// straight away on each change, never through requestAnimationFrame, and the layout is the static WASM one.
import { Button, Notice, useMediaQuery } from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../../i18n';
import { usePrefsStore } from '../../../settings/user/prefs';
import type { InviteNode } from '../../api';
import {
  fitView,
  hiddenBy,
  hitTest,
  indexGraph,
  lookOf,
  nodeRadius,
  subtreeSize,
  toggleCollapsed,
  zoomAt,
  type View,
} from './graphModel';
import { layoutGraph } from './layout';

const HEIGHT = 520;

/** The reduced-motion choice as ThemeProvider resolves it: the in-app setting wins over the OS one. */
export function useGraphReducedMotion(): boolean {
  const system = useMediaQuery('(prefers-reduced-motion: reduce)');
  const pref = usePrefsStore((s) => s.prefs.reducedMotion);
  return pref === 'on' || (pref === 'system' && system);
}

interface Props {
  root: InviteNode;
  nodes: InviteNode[];
  selected: string | null;
  onSelect: (accountId: string) => void;
}

export function GraphCanvas({ root, nodes, selected, onSelect }: Props) {
  const t = useT();
  const graph = useMemo(() => indexGraph(root, nodes), [root, nodes]);
  const [pos, setPos] = useState<Float32Array | null>(null);
  const [failed, setFailed] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [size, setSize] = useState({ w: 800, h: HEIGHT });
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const view = useRef<View>({ x: 0, y: 0, k: 1 });
  const frame = useRef(0);
  const hidden = useMemo(() => hiddenBy(graph, collapsed), [graph, collapsed]);
  const reduced = useGraphReducedMotion();

  // Layout in WASM (once per graph).
  useEffect(() => {
    let dead = false;
    setPos(null);
    setFailed(false);
    setCollapsed(new Set());
    layoutGraph(graph.nodes.length, graph.edges).then(
      (p) => !dead && setPos(p),
      () => !dead && setFailed(true),
    );
    return () => {
      dead = true;
    };
  }, [graph]);

  // Follow the container width.
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const read = () => setSize({ w: Math.max(320, Math.round(el.clientWidth || 800)), h: HEIGHT });
    read();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const draw = useCallback(() => {
    const el = canvas.current;
    const ctx = el?.getContext('2d');
    if (!el || !ctx || !pos) return;
    const dpr = window.devicePixelRatio || 1;
    el.width = Math.round(size.w * dpr);
    el.height = Math.round(size.h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);
    const css = getComputedStyle(el);
    const color = (token: string, fallback: string) =>
      css.getPropertyValue(token).trim() || fallback;
    const v = view.current;
    const sx = (i: number) => pos[2 * i]! * v.k + v.x;
    const sy = (i: number) => pos[2 * i + 1]! * v.k + v.y;

    ctx.lineWidth = 1;
    ctx.strokeStyle = color('--ui-border', '#555');
    ctx.globalAlpha = 0.6;
    ctx.beginPath();
    for (let i = 1; i < graph.nodes.length; i++) {
      const p = graph.parent[i]!;
      if (hidden[i] || hidden[p]) continue;
      ctx.moveTo(sx(p), sy(p));
      ctx.lineTo(sx(i), sy(i));
    }
    ctx.stroke();
    ctx.globalAlpha = 1;

    const labels = v.k >= 1.4;
    ctx.font = '11px sans-serif';
    ctx.textBaseline = 'middle';
    for (let i = 0; i < graph.nodes.length; i++) {
      if (hidden[i]) continue;
      const x = sx(i);
      const y = sy(i);
      if (x < -20 || y < -20 || x > size.w + 20 || y > size.h + 20) continue;
      const n = graph.nodes[i]!;
      const look = lookOf(n);
      const r = nodeRadius(graph.children[i]!.length) * Math.min(Math.max(v.k, 0.6), 2);
      if (look.halo > 0) {
        ctx.beginPath();
        ctx.arc(x, y, r + look.halo, 0, Math.PI * 2);
        ctx.strokeStyle = color('--ui-warning', '#e0a526');
        ctx.lineWidth = 2;
        ctx.stroke();
      }
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = color(look.token, look.fallback);
      ctx.fill();
      if (collapsed.has(n.account_id)) {
        // A collapsed node is ringed with a dashed line.
        ctx.strokeStyle = color('--ui-text', '#fff');
        ctx.lineWidth = 1.5;
        ctx.setLineDash([3, 2]);
        ctx.beginPath();
        ctx.arc(x, y, r + 3, 0, Math.PI * 2);
        ctx.stroke();
        ctx.setLineDash([]);
      }
      if (look.banned) {
        ctx.strokeStyle = color('--ui-bg', '#111');
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(x - r * 0.6, y - r * 0.6);
        ctx.lineTo(x + r * 0.6, y + r * 0.6);
        ctx.moveTo(x + r * 0.6, y - r * 0.6);
        ctx.lineTo(x - r * 0.6, y + r * 0.6);
        ctx.stroke();
      }
      if (n.account_id === selected) {
        ctx.strokeStyle = color('--ui-accent', '#c9a24d');
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(x, y, r + 5, 0, Math.PI * 2);
        ctx.stroke();
      }
      if (labels || n.account_id === selected) {
        ctx.fillStyle = color('--ui-text', '#eee');
        ctx.fillText(`@${n.username}`, x + r + 6, y);
      }
    }
  }, [graph, pos, hidden, collapsed, selected, size]);

  const schedule = useCallback(() => {
    if (frame.current) cancelAnimationFrame(frame.current);
    frame.current = 0;
    if (reduced) {
      // No frame loop: one static redraw per change.
      draw();
      return;
    }
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      draw();
    });
  }, [draw, reduced]);

  const fit = useCallback(() => {
    if (!pos) return;
    view.current = fitView(pos, hidden, size.w, size.h);
    schedule();
  }, [pos, hidden, size, schedule]);

  // Fit the first picture, and again when the width changes.
  useEffect(() => {
    fit();
  }, [pos, size.w]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    schedule();
    return () => {
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
  }, [schedule]);

  // Wheel zoom needs a non-passive listener to keep the page from scrolling.
  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      view.current = zoomAt(
        view.current,
        e.clientX - r.left,
        e.clientY - r.top,
        Math.exp(-e.deltaY * 0.0015),
      );
      schedule();
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [schedule, failed]);

  // Dragging pans; a press that did not move selects.
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const nodeAt = (e: { clientX: number; clientY: number }) => {
    if (!pos || !canvas.current) return -1;
    const r = canvas.current.getBoundingClientRect();
    return hitTest(graph, pos, hidden, view.current, e.clientX - r.left, e.clientY - r.top);
  };

  const collapseAt = (id: string) => setCollapsed((c) => toggleCollapsed(graph, c, id));
  const selectedIndex = selected ? (graph.index.get(selected) ?? -1) : -1;
  const hiddenCount = useMemo(() => hidden.reduce((n, h) => n + h, 0), [hidden]);

  const pan = (dx: number, dy: number) => {
    view.current = { ...view.current, x: view.current.x + dx, y: view.current.y + dy };
    schedule();
  };
  const zoom = (factor: number) => {
    view.current = zoomAt(view.current, size.w / 2, size.h / 2, factor);
    schedule();
  };

  if (failed)
    return (
      <Notice variant="warning" role="alert" className="ss-status">
        {t('admin.graph.canvasFailed')}
      </Notice>
    );
  return (
    <div className="adm-canvas" ref={wrap}>
      <div className="adm-actions" role="toolbar" aria-label={t('admin.graph.canvasTools')}>
        <Button size="sm" variant="secondary" onClick={() => zoom(1.4)}>
          {t('admin.graph.canvasZoomIn')}
        </Button>
        <Button size="sm" variant="secondary" onClick={() => zoom(1 / 1.4)}>
          {t('admin.graph.canvasZoomOut')}
        </Button>
        <Button size="sm" variant="secondary" onClick={fit}>
          {t('admin.graph.canvasFit')}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          disabled={selectedIndex < 0 || graph.children[selectedIndex]!.length === 0}
          onClick={() => selected && collapseAt(selected)}
        >
          {selected && collapsed.has(selected)
            ? t('admin.graph.canvasExpand', {
                n: selectedIndex >= 0 ? subtreeSize(graph, selectedIndex) : 0,
              })
            : t('admin.graph.canvasCollapse')}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          disabled={collapsed.size === 0}
          onClick={() => setCollapsed(new Set())}
        >
          {t('admin.graph.canvasExpandAll')}
        </Button>
        <span className="ss-meta" role="status">
          {pos
            ? t('admin.graph.canvasShown', {
                shown: graph.nodes.length - hiddenCount,
                total: graph.nodes.length,
              })
            : t('admin.graph.canvasLaying')}
        </span>
      </div>
      <canvas
        ref={canvas}
        className="adm-canvas-el"
        width={size.w}
        height={size.h}
        style={{ inlineSize: '100%', blockSize: size.h }}
        tabIndex={0}
        role="img"
        aria-label={t('admin.graph.canvasLabel', { n: graph.nodes.length })}
        data-reduced-motion={reduced ? 'true' : undefined}
        onPointerDown={(e) => {
          drag.current = { x: e.clientX, y: e.clientY, moved: false };
          e.currentTarget.setPointerCapture?.(e.pointerId);
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          const dx = e.clientX - d.x;
          const dy = e.clientY - d.y;
          if (!d.moved && Math.hypot(dx, dy) < 4) return;
          d.moved = true;
          d.x = e.clientX;
          d.y = e.clientY;
          pan(dx, dy);
        }}
        onPointerUp={(e) => {
          const d = drag.current;
          drag.current = null;
          if (d && !d.moved) {
            const hit = nodeAt(e);
            if (hit >= 0) onSelect(graph.ids[hit]!);
          }
        }}
        onDoubleClick={(e) => {
          const hit = nodeAt(e);
          if (hit >= 0) collapseAt(graph.ids[hit]!);
        }}
        onKeyDown={(e) => {
          const step = 40;
          if (e.key === 'ArrowLeft') pan(step, 0);
          else if (e.key === 'ArrowRight') pan(-step, 0);
          else if (e.key === 'ArrowUp') pan(0, step);
          else if (e.key === 'ArrowDown') pan(0, -step);
          else if (e.key === '+' || e.key === '=') zoom(1.4);
          else if (e.key === '-') zoom(1 / 1.4);
          else if (e.key === '0') fit();
          else if ((e.key === 'Enter' || e.key === ' ') && selected) collapseAt(selected);
          else return;
          e.preventDefault();
        }}
      />
      <p className="ss-meta">{t('admin.graph.canvasHelp')}</p>
    </div>
  );
}
