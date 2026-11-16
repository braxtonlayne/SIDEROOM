import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { Tabs } from './Tabs';

const items = ['a', 'b', 'c'].map((id) => ({ id, label: id, content: id }));

let size = { scrollWidth: 300, clientWidth: 100, scrollLeft: 0 };
const saved: Record<string, PropertyDescriptor | undefined> = {};

beforeEach(() => {
  size = { scrollWidth: 300, clientWidth: 100, scrollLeft: 0 };
  for (const k of ['scrollWidth', 'clientWidth', 'scrollLeft'] as const) {
    saved[k] = Object.getOwnPropertyDescriptor(HTMLElement.prototype, k);
    Object.defineProperty(HTMLElement.prototype, k, {
      configurable: true,
      get: () => size[k],
      set: (v: number) => {
        size[k] = v;
      },
    });
  }
});

afterEach(() => {
  for (const k of ['scrollWidth', 'clientWidth', 'scrollLeft']) {
    if (saved[k]) Object.defineProperty(HTMLElement.prototype, k, saved[k]!);
    else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[k];
  }
});

function scrollTo(list: HTMLElement, left: number) {
  size.scrollLeft = left;
  act(() => {
    fireEvent.scroll(list);
  });
}

describe('Tabs overflow cue (D-812)', () => {
  it('sets data-overflow from the scroll position', () => {
    render(<Tabs label="S" items={items} />);
    const list = screen.getByRole('tablist');
    expect(list).toHaveAttribute('data-overflow', 'end');
    scrollTo(list, 100);
    expect(list).toHaveAttribute('data-overflow', 'both');
    scrollTo(list, 200);
    expect(list).toHaveAttribute('data-overflow', 'start');
  });

  it('has no attribute when everything fits', () => {
    size.scrollWidth = 100;
    render(<Tabs label="S" items={items} />);
    expect(screen.getByRole('tablist')).not.toHaveAttribute('data-overflow');
  });
});
