import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Popover } from './Overlay/Overlay';

// Wave 77, SEC-AUD-301 (D-799): `Popover` can be placed against another element than its trigger.
// (On the base `anchor` is not a prop: the first case does not compile there.)

const rect = (left: number, top: number, width: number, height: number): DOMRect =>
  ({
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  }) as DOMRect;

afterEach(() => vi.restoreAllMocks());

function fakeRects() {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
    if (this.getAttribute('data-testid') === 'box') return rect(100, 500, 300, 60);
    // The popover's own span (the trigger's wrapper).
    if (this.contains(document.querySelector('[aria-haspopup]'))) return rect(10, 300, 40, 20);
    return rect(0, 0, 0, 0);
  });
}

function Harness({ useBox }: { useBox: boolean }) {
  const [box, setBox] = useState<HTMLDivElement | null>(null);
  return (
    <div>
      <div data-testid="box" ref={setBox} />
      <Popover
        label="Pick"
        placement="top-end"
        anchor={useBox ? box : undefined}
        trigger={<button>Open</button>}
      >
        <p>inside</p>
      </Popover>
    </div>
  );
}

describe('Popover anchor', () => {
  it('places the panel against the given anchor, not the trigger', async () => {
    fakeRects();
    render(<Harness useBox />);
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    const panel = screen.getByRole('dialog', { name: 'Pick' });
    // top-end against {100,500,300x60}: x = 100 + 300 - 0, y = 500 - 0 - 8.
    expect(panel.style.transform).toBe('translate(400px, 492px)');
  });

  it("follows the trigger's span when no anchor is given", async () => {
    fakeRects();
    render(<Harness useBox={false} />);
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    const panel = screen.getByRole('dialog', { name: 'Pick' });
    // top-end against {10,300,40x20}: x = 10 + 40, y = 300 - 8.
    expect(panel.style.transform).toBe('translate(50px, 292px)');
  });
});
