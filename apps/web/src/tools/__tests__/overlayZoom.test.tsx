import { act, fireEvent, render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { baseHandlers } from '../../lounge/__tests__/gapsHelpers';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { ThemedOverlayRoot } from '../../settings/user/ThemedOverlayRoot';
import { overlayZoom, zoomFactor } from '../../settings/user/overlayZoom';
import { GadgetLayer } from '../gadgets/GadgetLayer';
import { EMPTY_LAYOUT, dragRect, zoomedViewport, type DragGesture } from '../gadgets/layout';
import { useToolsStore, viewport } from '../gadgets/store';

const MINS = { minWidth: 240, minHeight: 160 };
const VP = { w: 1280, h: 800 };
const gesture = (over: Partial<DragGesture> = {}): DragGesture => ({
  kind: 'move',
  startX: 400,
  startY: 300,
  from: { x: 100, y: 100, w: 360, h: 420 },
  sign: 1,
  zoom: 1,
  ...over,
});
const setZoom = (zoom: number) =>
  act(() => usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, zoom } }));

afterEach(() => {
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useToolsStore.setState({ layout: EMPTY_LAYOUT, limitHit: false });
});

describe('the gadget drag under the member zoom (D-540)', () => {
  it('moves by the pointer delta at 100 %', () => {
    expect(dragRect(gesture(), 550, 360, VP, MINS)).toMatchObject({ x: 250, y: 160 });
  });

  it('divides the pointer delta by the zoom, so at 150 % the gadget follows the pointer', () => {
    // 150 viewport pixels is 100 of the layer's zoomed pixels, which the browser draws 150 pixels further on.
    const z = 1.5;
    const from = { x: 100, y: 100, w: 360, h: 200 };
    const r = dragRect(gesture({ zoom: z, from }), 550, 360, zoomedViewport(VP, z), MINS);
    expect(r).toMatchObject({ x: 200, y: 140 });
    // On screen the title bar moved exactly as far as the pointer did.
    expect((r.x - 100) * z).toBe(150);
    expect((r.y - 100) * z).toBe(60);
  });

  it('scales a resize the same way, and mirrors the inline axis in right-to-left', () => {
    const z = 2;
    const vp = zoomedViewport(VP, z);
    const from = { x: 0, y: 0, w: 360, h: 200 };
    // 100 and 40 viewport pixels at 200 % are 50 and 20 of the layer's.
    expect(dragRect(gesture({ kind: 'size', zoom: z, from }), 500, 340, vp, MINS)).toMatchObject({
      w: 410,
      h: 220,
    });
    expect(dragRect(gesture({ sign: -1, zoom: z }), 300, 300, vp, MINS)).toMatchObject({ x: 150 });
  });

  it('clamps to the layer, which has fewer pixels at a higher zoom', () => {
    const z = 2;
    const vp = zoomedViewport(VP, z);
    expect(vp).toEqual({ w: 640, h: 400 });
    // Dragged far to the end: the gadget stops with its end edge at the layer's (visible) end.
    const r = dragRect(
      gesture({ zoom: z, from: { x: 100, y: 10, w: 300, h: 200 } }),
      5000,
      300,
      vp,
      MINS,
    );
    expect(r.x + r.w).toBe(640);
  });

  it('treats a missing or nonsense zoom as 100 %', () => {
    expect(zoomedViewport(VP, 0)).toEqual(VP);
    expect(dragRect(gesture({ zoom: Number.NaN }), 550, 360, VP, MINS)).toMatchObject({ x: 250 });
    expect(zoomFactor(undefined)).toBe(1);
    expect(zoomFactor(150)).toBe(1.5);
  });

  it('the store measures the window in the zoomed pixels the saved zoom gives', () => {
    setZoom(150);
    expect(overlayZoom()).toBe(1.5);
    expect(viewport().w).toBeCloseTo(window.innerWidth / 1.5);
    expect(viewport().h).toBeCloseTo(window.innerHeight / 1.5);
  });
});

describe('the overlay root carries the zoom (D-540)', () => {
  beforeEach(() => {
    baseHandlers();
  });

  it('sets --sr-overlay-zoom on the box-less root instead of a real zoom, and follows the setting', async () => {
    const { container } = render(
      <ThemedOverlayRoot className="sr-tl-theme">
        <div data-testid="child" />
      </ThemedOverlayRoot>,
    );
    await screen.findByTestId('child');
    const root = () => container.querySelector<HTMLElement>('.sr-overlay-root.sr-tl-theme')!;
    expect(root()).not.toBeNull();
    expect(root().style.getPropertyValue('--sr-overlay-zoom')).toBe('1');
    setZoom(150);
    expect(root().style.getPropertyValue('--sr-overlay-zoom')).toBe('1.5');
    // No real zoom: on the box-less root it would do nothing, and a box would restack the overlays.
    expect(root().getAttribute('style') ?? '').not.toMatch(/(^|;)\s*zoom:/);
    expect(root().style.display).toBe('contents');
  });

  it('applies the property to every positioned child of the root, and only through it', () => {
    const css = readFileSync(join(__dirname, '../../settings/user/overlay-root.css'), 'utf8');
    expect(css).toMatch(/\.sr-overlay-root > \*\s*\{\s*zoom: var\(--sr-overlay-zoom, 1\);\s*\}/);
  });

  it('a gadget dragged at 150 % stores the delta in the layer pixels', async () => {
    setZoom(150);
    useToolsStore.setState({
      layout: { gadgets: [{ tool: 'dice', x: 60, y: 60, w: 320, h: 360, z: 1 }], pins: [] },
    });
    const proto = HTMLElement.prototype as unknown as { setPointerCapture?: unknown };
    const had = proto.setPointerCapture;
    proto.setPointerCapture = () => {};
    // jsdom has no PointerEvent: without one the pointer events reach React with no coordinates.
    const w = window as unknown as { PointerEvent?: unknown };
    const hadPointer = w.PointerEvent;
    if (!hadPointer)
      w.PointerEvent = class extends MouseEvent {
        pointerId: number;
        constructor(type: string, init: PointerEventInit = {}) {
          super(type, init);
          this.pointerId = init.pointerId ?? 1;
        }
      };
    try {
      render(
        <ThemedOverlayRoot className="sr-tl-theme">
          <GadgetLayer />
        </ThemedOverlayRoot>,
      );
      const head = await screen.findByRole('toolbar', { name: /dice/i });
      fireEvent.pointerDown(head, { button: 0, clientX: 200, clientY: 200, pointerId: 1 });
      fireEvent.pointerMove(head, { clientX: 290, clientY: 245, pointerId: 1 });
      fireEvent.pointerUp(head, { clientX: 290, clientY: 245, pointerId: 1 });
      const g = useToolsStore.getState().layout.gadgets[0]!;
      expect(g).toMatchObject({ x: 60 + 60, y: 60 + 30 });
    } finally {
      proto.setPointerCapture = had;
      w.PointerEvent = hadPointer;
    }
  });
});

describe('popovers of the app open above dialogs (wave 51)', () => {
  const zOf = (file: string, selector: string) => {
    const css = readFileSync(join(__dirname, '../..', file), 'utf8');
    const start = css.indexOf(`${selector} {`);
    expect(start).toBeGreaterThanOrEqual(0);
    return Number(/z-index:\s*(\d+)/.exec(css.slice(start, css.indexOf('}', start)))![1]);
  };
  it('the profile popout and the TV programme popover sit above the modal backdrop (1000), below toasts', () => {
    for (const z of [
      zOf('lounge/profile/profile.css', '.lng-pop'),
      zOf('cinema/tv/tv.css', '.tv-pop'),
    ]) {
      expect(z).toBeGreaterThan(1000);
      expect(z).toBeLessThan(1100);
    }
  });
});
