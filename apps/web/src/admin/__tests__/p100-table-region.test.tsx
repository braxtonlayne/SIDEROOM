// SEC-AUD-441 (D-949): TableRegion is a named region and a tab stop only while its table is wider than it.
import { act, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TableRegion } from '../TableRegion';

type Cb = () => void;
let callbacks: Cb[] = [];

function stubObserver() {
  callbacks = [];
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(cb: Cb) {
        callbacks.push(cb);
      }
      observe() {}
      disconnect() {}
    },
  );
}

function stubWidths(scrollWidth: number, clientWidth: number) {
  vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockReturnValue(scrollWidth);
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(clientWidth);
}

const table = (
  <table aria-label="Things">
    <tbody>
      <tr>
        <td>x</td>
      </tr>
    </tbody>
  </table>
);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('TableRegion', () => {
  it('is a plain wrapper while the table fits', () => {
    stubObserver();
    stubWidths(500, 500);
    const { container } = render(<TableRegion label="Things">{table}</TableRegion>);
    const wrap = container.firstElementChild as HTMLElement;
    expect(wrap.className).toBe('adm-table-wrap');
    expect(wrap.hasAttribute('role')).toBe(false);
    expect(wrap.hasAttribute('tabindex')).toBe(false);
    expect(wrap.hasAttribute('aria-label')).toBe(false);
    expect(wrap.firstElementChild?.tagName).toBe('TABLE');
  });

  it('is a named region and a tab stop while the table overflows, and plain again when it fits', () => {
    stubObserver();
    stubWidths(700, 500);
    const { container } = render(
      <TableRegion label="Things" className="extra">
        {table}
      </TableRegion>,
    );
    const wrap = container.firstElementChild as HTMLElement;
    expect(wrap.className).toBe('adm-table-wrap extra');
    expect(screen.getByRole('region', { name: 'Things' })).toBe(wrap);
    expect(wrap.getAttribute('tabindex')).toBe('0');
    stubWidths(400, 500);
    act(() => callbacks.forEach((cb) => cb()));
    expect(wrap.hasAttribute('role')).toBe(false);
    expect(wrap.hasAttribute('tabindex')).toBe(false);
  });

  it('is a plain wrapper where there is no ResizeObserver', () => {
    vi.stubGlobal('ResizeObserver', undefined);
    stubWidths(700, 500);
    const { container } = render(<TableRegion label="Things">{table}</TableRegion>);
    expect((container.firstElementChild as HTMLElement).hasAttribute('tabindex')).toBe(false);
  });
});
