import { act, render } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { effectiveZoom, useFloating } from './useFloating';

describe('effectiveZoom (D-540)', () => {
  it("reads the element's CSS zoom where the browser reports it", () => {
    const el = document.createElement('div');
    Object.defineProperty(el, 'currentCSSZoom', { value: 1.5 });
    expect(effectiveZoom(el)).toBe(1.5);
  });

  it('is 1 without an element, without the property, or with a nonsense value', () => {
    expect(effectiveZoom(null)).toBe(1);
    expect(effectiveZoom(document.createElement('div'))).toBe(1);
    const bad = document.createElement('div');
    Object.defineProperty(bad, 'currentCSSZoom', { value: 0 });
    expect(effectiveZoom(bad)).toBe(1);
  });
});

function Floating({ x, y }: { x: number; y: number }) {
  const f = useFloating({
    open: true,
    anchor: { x, y },
    placement: 'bottom-start',
    offset: 0,
    flip: false,
    shift: false,
  });
  return <div data-testid="f" ref={f.ref} style={f.style} />;
}

describe('useFloating under a zoomed box (D-540)', () => {
  afterEach(() => {
    delete (HTMLElement.prototype as { currentCSSZoom?: number }).currentCSSZoom;
  });

  it("places the element at the anchor's viewport point at 100 %", () => {
    const { getByTestId } = render(<Floating x={300} y={120} />);
    expect(getByTestId('f').style.transform).toBe('translate(300px, 120px)');
  });

  it('divides the viewport point by the zoom, because the translate is scaled by it', () => {
    Object.defineProperty(HTMLElement.prototype, 'currentCSSZoom', {
      configurable: true,
      get: () => 1.5,
    });
    const { getByTestId } = render(<Floating x={300} y={120} />);
    expect(getByTestId('f').style.transform).toBe('translate(200px, 80px)');
  });
});

describe('the --ui-zoom token (D-555)', () => {
  it('is the factor as a string, and 1 for nonsense', async () => {
    const { zoomVars, UI_ZOOM_TOKEN } = await import('../../theme/runtime');
    expect(UI_ZOOM_TOKEN).toBe('--ui-zoom');
    expect(zoomVars(1.5)).toEqual({ '--ui-zoom': '1.5' });
    expect(zoomVars(0)).toEqual({ '--ui-zoom': '1' });
    expect(zoomVars(Number.NaN)).toEqual({ '--ui-zoom': '1' });
  });

  it('a dialog inside a zoomed box divides its max-height by it', () => {
    const css = readFileSync(join(__dirname, '../Modal/Modal.module.css'), 'utf8');
    expect(css).toContain('max-height: calc(100dvh / var(--ui-zoom, 1) - 32px);');
  });

  it("a dialog's width never comes from its content (XC-VIS-004)", () => {
    const css = readFileSync(join(__dirname, '../Modal/Modal.module.css'), 'utf8');
    // The backdrop's one grid track may shrink below the dialog's min-content, and the dialog may shrink with it.
    expect(css).toMatch(/\.backdrop\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\);/);
    expect(css).toMatch(/\.dialog\s*{[^}]*min-width:\s*0;/);
  });
});

describe('useFloating with an anchor that has no box (D-870)', () => {
  const box = { x: 989, y: 316, width: 28, height: 28 };
  function Harness({ anchor }: { anchor: HTMLElement }) {
    const [, force] = useState(0);
    const f = useFloating({ open: true, anchor, placement: 'bottom-end', offset: 4 });
    return (
      <div>
        <button onClick={() => force((n) => n + 1)}>again</button>
        <div data-testid="f" ref={f.ref} style={f.style} />
      </div>
    );
  }
  function anchorWith(rect: () => typeof box) {
    const a = document.createElement('span');
    document.body.appendChild(a);
    Object.defineProperty(a, 'checkVisibility', { value: () => true });
    a.getBoundingClientRect = () => {
      const r = rect();
      return {
        ...r,
        left: r.x,
        top: r.y,
        right: r.x + r.width,
        bottom: r.y + r.height,
        toJSON() {},
      };
    };
    return a;
  }

  it('keeps the place it has when the anchor measures 0 x 0, and stays hidden when it has none', async () => {
    vi.useFakeTimers();
    let current = { x: 0, y: 0, width: 0, height: 0 };
    const a = anchorWith(() => current);
    const { getByTestId, rerender } = render(<Harness anchor={a} />);
    // No box at the first measure: nothing is placed (not the corner), the element is hidden.
    expect(getByTestId('f').style.transform).toBe('');
    expect(getByTestId('f').style.visibility).toBe('hidden');
    // The anchor gets its box on a later frame: it is placed there.
    current = box;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(40);
    });
    const placed = getByTestId('f').style.transform;
    expect(placed.startsWith('translate(')).toBe(true);
    expect(placed).not.toBe('translate(8px, 4px)');
    // The box is lost again (a display: none ancestor): a new measure keeps the last position.
    current = { x: 0, y: 0, width: 0, height: 0 };
    rerender(<Harness anchor={a} />);
    window.dispatchEvent(new Event('resize'));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });
    expect(getByTestId('f').style.transform).toBe(placed);
    vi.useRealTimers();
    a.remove();
  });
});
