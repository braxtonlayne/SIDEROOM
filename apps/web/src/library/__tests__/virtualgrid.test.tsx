// The Browse windowed grid (LIB-BRW-001): its pure helpers and the fallback where nothing can be measured.
import { render, screen } from '@testing-library/react';
import { columnsFor, VIRTUAL_MIN_ITEMS, VirtualGrid, visibleRows } from '../VirtualGrid';
import { csvList, nextState } from '../BrowseFilters';

describe('virtual grid helpers', () => {
  it('fits as many 150 px columns as the width allows', () => {
    expect(columnsFor(0)).toBe(1);
    expect(columnsFor(150)).toBe(1);
    expect(columnsFor(320)).toBe(2);
    expect(columnsFor(1000)).toBe(6);
  });

  it('mounts the rows in view plus overscan, clamped to the list', () => {
    // 100 rows of 350 px; viewing rows 10 to 12.
    expect(visibleRows(3500, 800, 350, 100)).toEqual({ first: 7, last: 16 });
    expect(visibleRows(0, 800, 350, 100)).toEqual({ first: 0, last: 6 });
    expect(visibleRows(34_900, 800, 350, 100)).toEqual({ first: 96, last: 100 });
    expect(visibleRows(0, 800, 350, 2)).toEqual({ first: 0, last: 2 });
    expect(visibleRows(0, 800, 350, 0)).toEqual({ first: 0, last: 0 });
  });

  it('cycles genre chips and parses lists', () => {
    expect(nextState('off')).toBe('include');
    expect(nextState('include')).toBe('exclude');
    expect(nextState('exclude')).toBe('off');
    expect(csvList(' Action, fantasy ,,')).toEqual(['action', 'fantasy']);
  });
});

describe('VirtualGrid', () => {
  it('renders every item where nothing can be measured (no layout)', () => {
    const items = Array.from({ length: VIRTUAL_MIN_ITEMS + 40 }, (_, i) => ({ id: String(i) }));
    render(
      <VirtualGrid items={items} keyOf={(x) => x.id} render={(x) => <span>item {x.id}</span>} />,
    );
    expect(screen.getByTestId('browse-grid')).not.toHaveAttribute('data-virtual');
    expect(screen.getAllByText(/^item /)).toHaveLength(items.length);
  });
});
