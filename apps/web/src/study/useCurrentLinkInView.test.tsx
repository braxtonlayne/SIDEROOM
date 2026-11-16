// D-821: one hook brings the current link of a scrolling bar into view by moving the bar's own scrollLeft.
import { render } from '@testing-library/react';
import { useRef } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useCurrentLinkInView } from './useCurrentLinkInView';

const SEL = "[aria-current='page']";

function Bar({ k }: { k: string }) {
  const ref = useRef<HTMLElement>(null);
  useCurrentLinkInView(ref, SEL, k);
  return (
    <nav ref={ref} data-testid="bar">
      <a href="#a">A</a>
      <a href="#b" aria-current="page">
        B
      </a>
    </nav>
  );
}

function rect(left: number, right: number): DOMRect {
  return {
    left,
    right,
    top: 0,
    bottom: 10,
    width: right - left,
    height: 10,
    x: left,
    y: 0,
    toJSON: () => ({}),
  };
}

/** Fake a 300 px wide bar at x = 0 holding 600 px of content, and a link at the given box. */
function fake(bar: HTMLElement, left: number, right: number, scrollLeft = 0): HTMLElement {
  const link = bar.querySelector<HTMLElement>(SEL)!;
  Object.defineProperty(bar, 'scrollWidth', { configurable: true, value: 600 });
  Object.defineProperty(bar, 'clientWidth', { configurable: true, value: 300 });
  bar.scrollLeft = scrollLeft;
  bar.getBoundingClientRect = () => rect(0, 300);
  link.getBoundingClientRect = () => rect(left, right);
  return link;
}

afterEach(() => vi.restoreAllMocks());

describe('useCurrentLinkInView', () => {
  it('leaves scrollLeft alone when the link is inside the bar', () => {
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    const bar = getByTestId('bar');
    fake(bar, 100, 180, 50);
    rerender(<Bar k="/b" />);
    expect(bar.scrollLeft).toBe(50);
  });

  it('moves the bar forward by the difference when the link is past the end', () => {
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    const bar = getByTestId('bar');
    fake(bar, 280, 360);
    rerender(<Bar k="/b" />);
    expect(bar.scrollLeft).toBeGreaterThanOrEqual(60);
    expect(bar.scrollLeft).toBeLessThanOrEqual(60 + 16);
  });

  it('moves the bar back when the link is before the start', () => {
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    const bar = getByTestId('bar');
    fake(bar, -90, -10, 200);
    rerender(<Bar k="/b" />);
    expect(bar.scrollLeft).toBeLessThanOrEqual(110);
    expect(bar.scrollLeft).toBeGreaterThanOrEqual(110 - 16);
  });

  it('never calls scrollIntoView', () => {
    const spy = vi.fn();
    Element.prototype.scrollIntoView = spy;
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    fake(getByTestId('bar'), 280, 360);
    rerender(<Bar k="/b" />);
    expect(spy).not.toHaveBeenCalled();
  });

  it('runs again when key changes, and not when it stays', () => {
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    const bar = getByTestId('bar');
    fake(bar, 280, 360);
    rerender(<Bar k="/a" />);
    expect(bar.scrollLeft).toBe(0);
    rerender(<Bar k="/b" />);
    expect(bar.scrollLeft).toBeGreaterThan(0);
  });

  it('moves the right way in a right-to-left bar (boxes, not offsets)', () => {
    const { getByTestId, rerender } = render(<Bar k="/a" />);
    const bar = getByTestId('bar');
    bar.setAttribute('dir', 'rtl');
    // In rtl the start is on the right; a link past the left edge needs scrollLeft to go down (negative side).
    fake(bar, -90, -10, -20);
    rerender(<Bar k="/b" />);
    expect(bar.scrollLeft).toBeLessThan(-20);
  });
});
