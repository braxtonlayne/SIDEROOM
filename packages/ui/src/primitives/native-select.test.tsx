import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createRef } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../test-axe';
import { NativeSelect } from './Select/NativeSelect';

const options = (
  <>
    <option value="a">Alpha</option>
    <option value="b">Beta</option>
    <option value="c">Gamma</option>
  </>
);

describe('NativeSelect (D-950)', () => {
  it('renders a real select with its options and merges className', () => {
    render(
      <NativeSelect aria-label="Letter" className="extra" defaultValue="b">
        {options}
      </NativeSelect>,
    );
    const box = screen.getByRole('combobox', { name: 'Letter' });
    expect(box.tagName).toBe('SELECT');
    expect(box.className).toContain('extra');
    expect(screen.getAllByRole('option')).toHaveLength(3);
    expect(box).toHaveValue('b');
  });

  it('forwards the ref to the select', () => {
    const ref = createRef<HTMLSelectElement>();
    render(
      <NativeSelect aria-label="Letter" ref={ref}>
        {options}
      </NativeSelect>,
    );
    expect(ref.current).toBeInstanceOf(HTMLSelectElement);
    expect(ref.current).toBe(screen.getByRole('combobox'));
  });

  it('fires onChange with the new value', async () => {
    const onChange = vi.fn((e: React.ChangeEvent<HTMLSelectElement>) => e.target.value);
    render(
      <NativeSelect aria-label="Letter" onChange={onChange}>
        {options}
      </NativeSelect>,
    );
    await userEvent.selectOptions(screen.getByRole('combobox'), 'c');
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.results[0]?.value).toBe('c');
  });

  it('is disabled when asked', async () => {
    const onChange = vi.fn();
    render(
      <NativeSelect aria-label="Letter" disabled onChange={onChange}>
        {options}
      </NativeSelect>,
    );
    const box = screen.getByRole('combobox');
    expect(box).toBeDisabled();
    await userEvent.selectOptions(box, 'b');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('invalid sets aria-invalid', () => {
    const { rerender } = render(
      <NativeSelect aria-label="Letter" invalid>
        {options}
      </NativeSelect>,
    );
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-invalid', 'true');
    rerender(<NativeSelect aria-label="Letter">{options}</NativeSelect>);
    expect(screen.getByRole('combobox')).not.toHaveAttribute('aria-invalid');
  });

  it('multiple and size pass through and get no chevron wrapper', () => {
    const { container } = render(
      <NativeSelect aria-label="Letters" multiple size={3}>
        {options}
      </NativeSelect>,
    );
    const box = screen.getByRole('listbox');
    expect(box).toHaveAttribute('multiple');
    expect(box).toHaveAttribute('size', '3');
    expect(container.firstElementChild).toBe(box);
  });

  it('passes axe with a label', async () => {
    const { container } = render(
      <div>
        <label htmlFor="n-sel">Letter</label>
        <NativeSelect id="n-sel">{options}</NativeSelect>
      </div>,
    );
    expect(await axeViolations(container)).toEqual([]);
  });
});
