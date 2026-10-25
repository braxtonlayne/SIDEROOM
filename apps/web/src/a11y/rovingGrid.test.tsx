import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RovingScope } from './RovingNav';
import { columnsOf, nextIndex } from './useRovingTabindex';
import { mockGridRows, tabStops } from './rovingTestUtils';

/** RDY-A11Y-002: the grid mode of the roving hook and the RovingScope wrapper that the card grids use. */
describe('grid navigation', () => {
  it('moves by row with up and down and by one with left and right', () => {
    // 3 columns, 8 items: rows [0 1 2] [3 4 5] [6 7]
    expect(nextIndex('ArrowRight', 1, 8, 'grid', 3)).toBe(2);
    expect(nextIndex('ArrowDown', 1, 8, 'grid', 3)).toBe(4);
    expect(nextIndex('ArrowUp', 4, 8, 'grid', 3)).toBe(1);
    expect(nextIndex('ArrowUp', 1, 8, 'grid', 3)).toBe(1);
    // the last row is short: down from item 5 (no item 8) lands on the last item
    expect(nextIndex('ArrowDown', 5, 8, 'grid', 3)).toBe(7);
    expect(nextIndex('ArrowDown', 7, 8, 'grid', 3)).toBe(7);
    expect(nextIndex('Home', 4, 8, 'grid', 3)).toBe(0);
    expect(nextIndex('End', 4, 8, 'grid', 3)).toBe(7);
    expect(nextIndex('a', 4, 8, 'grid', 3)).toBeNull();
    expect(nextIndex('ArrowDown', 0, 0, 'grid', 3)).toBeNull();
  });

  it('reads the column count from the item positions', () => {
    const restore = mockGridRows('button', 4);
    render(
      <div>
        {Array.from({ length: 9 }, (_, i) => (
          <button key={i}>{i}</button>
        ))}
      </div>,
    );
    expect(columnsOf(screen.getAllByRole('button'))).toBe(4);
    expect(columnsOf([])).toBe(1);
    restore();
  });
});

describe('RovingScope', () => {
  it('turns a grid of links into one Tab stop with row-wise arrows', async () => {
    const restore = mockGridRows('a', 3);
    const user = userEvent.setup();
    render(
      <>
        <button>before</button>
        <RovingScope selector="a" orientation="grid">
          <ul>
            {Array.from({ length: 7 }, (_, i) => (
              <li key={i}>
                <a href={`#${i}`}>card {i}</a>
              </li>
            ))}
          </ul>
        </RovingScope>
        <button>after</button>
      </>,
    );
    const cards = screen.getAllByRole('link');
    expect(tabStops(cards)).toEqual([0, -1, -1, -1, -1, -1, -1]);
    await user.tab();
    await user.tab();
    expect(cards[0]).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(cards[3]).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    expect(cards[4]).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(cards[6]).toHaveFocus();
    expect(tabStops(cards)).toEqual([-1, -1, -1, -1, -1, -1, 0]);
    await user.keyboard('{ArrowUp}{Home}');
    expect(cards[0]).toHaveFocus();
    await user.keyboard('{End}');
    expect(cards[6]).toHaveFocus();
    // Tab leaves the grid in one step and Shift+Tab comes back to the last card.
    await user.tab();
    expect(screen.getByRole('button', { name: 'after' })).toHaveFocus();
    await user.tab({ shift: true });
    expect(cards[6]).toHaveFocus();
    restore();
  });

  it('leaves the layout alone (display: contents)', () => {
    const { container } = render(
      <RovingScope selector="a">
        <a href="#x">x</a>
      </RovingScope>,
    );
    expect((container.firstElementChild as HTMLElement).style.display).toBe('contents');
  });
});
