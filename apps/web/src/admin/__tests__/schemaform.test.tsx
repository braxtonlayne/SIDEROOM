import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import {
  defaultsOf,
  overridesOf,
  SchemaForm,
  type SchemaField,
  type SettingsValues,
} from '../SchemaForm';

const rowFields: SchemaField[] = [
  { key: 'emoji', label: 'Emoji', type: 'string' },
  { key: 'role', label: 'Role', type: 'role_picker', multi: false },
  { key: 'n', label: 'Count', type: 'int', default: 2 },
];
const fields: SchemaField[] = [
  {
    key: 'panels',
    label: 'Panels',
    type: 'list',
    max_items: 2,
    item: { type: 'group', fields: rowFields },
  },
  {
    key: 'modes',
    label: 'Modes',
    type: 'list',
    item: {
      type: 'enum',
      options: [
        { value: 'a', label: 'Mode A' },
        { value: 'b', label: 'Mode B' },
      ],
    },
  },
];

function Harness({ onValues }: { onValues: (v: SettingsValues) => void }) {
  const [values, setValues] = useState<SettingsValues>({});
  return (
    <SchemaForm
      fields={fields}
      values={values}
      pickers={{ channels: [], roles: [{ id: '7', name: 'Gamers' }] }}
      onChange={(v) => {
        setValues(v);
        onValues(v);
      }}
    />
  );
}

describe('SchemaForm lists', () => {
  it('seeds new rows with the defaults and prunes empty keys when saving', () => {
    expect(defaultsOf(rowFields)).toEqual({ n: 2 });
    expect(overridesOf(fields, { panels: [{ emoji: 'x', role: '', n: null }] })).toEqual({
      panels: [{ emoji: 'x' }],
    });
    expect(overridesOf(fields, { panels: [] })).toEqual({});
    expect(overridesOf(fields, { modes: ['a'] })).toEqual({ modes: ['a'] });
  });

  it('adds rows up to the schema limit, picks a role and toggles enum options', async () => {
    let last: SettingsValues = {};
    render(<Harness onValues={(v) => (last = v)} />);
    const add = screen.getByRole('button', { name: 'Add to Panels' });
    await userEvent.click(add);
    await userEvent.click(add);
    expect(add).toBeDisabled();
    expect(last.panels).toEqual([{ n: 2 }, { n: 2 }]);

    await userEvent.click(screen.getAllByRole('combobox', { name: 'Role' })[0]!);
    await userEvent.click(await screen.findByRole('option', { name: 'Gamers' }));
    expect((last.panels as { role?: string }[])[0]!.role).toBe('7');

    await userEvent.click(screen.getByRole('checkbox', { name: 'Mode B' }));
    expect(last.modes).toEqual(['b']);
    await userEvent.click(screen.getByRole('checkbox', { name: 'Mode B' }));
    expect(last.modes).toEqual([]);

    await userEvent.click(screen.getByRole('button', { name: 'Remove Panels 1' }));
    expect(screen.getAllByRole('group', { name: /Panels \d/ })).toHaveLength(1);
  });
});
