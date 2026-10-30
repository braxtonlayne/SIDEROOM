import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { en } from '../../../i18n/messages/en';
import { http, HttpResponse, server } from '../../../test/server';
import { configureToolsWasm, getToolsEngine, setToolsEngine } from '../engine';
import { ERROR_CODES, errorKey } from '../errors';
import { UNIT_NAME_KEYS } from '../unitNames';
import CalculatorTool from '../CalculatorTool';
import CurrencyTool from '../CurrencyTool';
import PercentTool from '../PercentTool';
import ProgrammerTool from '../ProgrammerTool';
import UnitsTool from '../UnitsTool';

// The tools run the real Rust engine (sr-tools-wasm). Needs `pnpm --filter @sideroom/wasm build`; vitest runs
// with apps/web as the working directory.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/tools/tools_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const props = { host: 'drawer' as const };

beforeAll(() => {
  if (built) configureToolsWasm(readFileSync(wasmPath));
});
beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  setToolsEngine(null);
});

describe.skipIf(!built)('the calculator (UTL-CALC-001)', () => {
  it('answers while you type and keeps its history', async () => {
    const user = userEvent.setup();
    render(<CalculatorTool {...props} />);
    const box = screen.getByRole('textbox', { name: 'Expression' });
    await user.type(box, '2*(3+4)');
    expect(await screen.findByText('14')).toBeInTheDocument();
    await user.keyboard('{Enter}');
    expect(
      within(screen.getByRole('region', { name: 'History' })).getByText(/= 14/),
    ).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem('sr.tools.calc.history')!)[0]).toMatchObject({
      expr: '2*(3+4)',
      text: '14',
    });
  });

  it('uses the previous answer and degrees or radians', async () => {
    const user = userEvent.setup();
    render(<CalculatorTool {...props} />);
    const box = screen.getByRole('textbox', { name: 'Expression' });
    await user.type(box, '7*6{Enter}');
    await user.clear(box);
    await user.type(box, 'ans+1');
    expect(await screen.findByText('43')).toBeInTheDocument();
    await user.clear(box);
    await user.type(box, 'sin(30)');
    expect(await screen.findByText('0.5')).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'Rad' }));
    expect(await screen.findByText('-0.988031624093')).toBeInTheDocument();
  });

  it('says what is wrong in words, with the position', async () => {
    const user = userEvent.setup();
    render(<CalculatorTool {...props} />);
    const box = screen.getByRole('textbox', { name: 'Expression' });
    await user.type(box, '1/0');
    expect(await screen.findByText('Division by zero.')).toBeInTheDocument();
    await user.clear(box);
    await user.type(box, '2 $ 3');
    expect(await screen.findByText('Unexpected character $ at position 3.')).toBeInTheDocument();
    await user.clear(box);
    await user.type(box, 'foo+1');
    expect(await screen.findByText('Unknown name: foo.')).toBeInTheDocument();
    // Nothing is added to the history by Enter on an error.
    await user.keyboard('{Enter}');
    expect(screen.getByText('Nothing yet.')).toBeInTheDocument();
  });

  it('types from the keypad at the caret and clears', async () => {
    const user = userEvent.setup();
    render(<CalculatorTool {...props} />);
    const box = screen.getByRole('textbox', { name: 'Expression' }) as HTMLInputElement;
    await user.click(screen.getByRole('button', { name: 'Square root' }));
    await user.type(box, '16)');
    expect(box.value).toBe('sqrt(16)');
    expect(await screen.findByText('4')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear' }));
    expect(box.value).toBe('');
  });

  it('reuses and clears history entries', async () => {
    const user = userEvent.setup();
    localStorage.setItem(
      'sr.tools.calc.history',
      JSON.stringify([{ expr: '5!', text: '120', value: 120 }]),
    );
    render(<CalculatorTool {...props} />);
    await user.click(screen.getByRole('button', { name: /5!/ }));
    expect((screen.getByRole('textbox', { name: 'Expression' }) as HTMLInputElement).value).toBe(
      '5!',
    );
    await user.click(screen.getByRole('button', { name: 'Clear history' }));
    expect(screen.getByText('Nothing yet.')).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem('sr.tools.calc.history')!)).toEqual([]);
  });

  it('ignores a broken stored history', () => {
    localStorage.setItem('sr.tools.calc.history', '{"oops":1}');
    render(<CalculatorTool {...props} />);
    expect(screen.getByText('Nothing yet.')).toBeInTheDocument();
  });

  it('is accessible', async () => {
    const { container } = render(<CalculatorTool {...props} />);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('says so when the engine cannot load', async () => {
    setToolsEngine(new Error('offline'));
    render(<CalculatorTool {...props} />);
    expect(await screen.findByRole('alert')).toHaveTextContent(/could not be loaded/);
  });
});

describe.skipIf(!built)('programmer mode', () => {
  it('shows every base, switches between signed and unsigned and warns on wrap-around', async () => {
    const user = userEvent.setup();
    render(<ProgrammerTool {...props} />);
    await user.click(screen.getByRole('radio', { name: '8' }));
    const box = screen.getByRole('textbox', { name: 'Expression' });
    await user.type(box, '0xFF');
    expect(await screen.findByText('FF')).toBeInTheDocument();
    expect(screen.getByText('-1')).toBeInTheDocument();
    expect(screen.getByText('255')).toBeInTheDocument();
    expect(screen.getByText('377')).toBeInTheDocument();
    expect(screen.getByText('1111 1111')).toBeInTheDocument();
    await user.click(screen.getByRole('checkbox', { name: /Signed/ }));
    await user.clear(box);
    await user.type(box, '255+1');
    expect(await screen.findByText(/wrapped around/)).toBeInTheDocument();
  });

  it('flips a bit and reads digits in the chosen base', async () => {
    const user = userEvent.setup();
    render(<ProgrammerTool {...props} />);
    await user.click(screen.getByRole('radio', { name: '8' }));
    const box = screen.getByRole('textbox', { name: 'Expression' }) as HTMLInputElement;
    await user.type(box, '0');
    await user.click(screen.getByRole('button', { name: 'Bit 7' }));
    expect(box.value).toBe('0x80');
    expect(await screen.findByText('1000 0000')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Bit 0' }));
    expect(box.value).toBe('0x81');
    await user.selectOptions(screen.getByLabelText('Digits are read as'), '2');
    await user.clear(box);
    await user.type(box, '1010');
    expect(await screen.findByText('0000 1010')).toBeInTheDocument();
  });

  it('explains a digit that does not exist in the base', async () => {
    const user = userEvent.setup();
    render(<ProgrammerTool {...props} />);
    await user.selectOptions(screen.getByLabelText('Digits are read as'), '8');
    await user.type(screen.getByRole('textbox', { name: 'Expression' }), '19');
    expect(
      await screen.findByText('The digit 9 does not exist in this base (position 2).'),
    ).toBeInTheDocument();
  });

  it('is accessible', async () => {
    const { container } = render(<ProgrammerTool {...props} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe.skipIf(!built)('units', () => {
  it('converts between two units and swaps them', async () => {
    const user = userEvent.setup();
    render(<UnitsTool {...props} />);
    expect(await screen.findByText('0.621371192237')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Swap the units' }));
    expect(await screen.findByText('1.609344')).toBeInTheDocument();
    await user.clear(screen.getByLabelText('Value'));
    await user.type(screen.getByLabelText('Value'), '10');
    expect(await screen.findByText('16.09344')).toBeInTheDocument();
  });

  it('converts temperatures and refuses below absolute zero', async () => {
    const user = userEvent.setup();
    render(<UnitsTool {...props} />);
    await user.selectOptions(screen.getByLabelText('Category'), 'temperature');
    const value = screen.getByLabelText('Value');
    await user.clear(value);
    await user.type(value, '100');
    expect(await screen.findByText('212')).toBeInTheDocument();
    await user.clear(value);
    await user.type(value, '-300');
    expect(await screen.findByText('That temperature is below absolute zero.')).toBeInTheDocument();
  });

  it('understands "5 km to mi" and applies it', async () => {
    const user = userEvent.setup();
    render(<UnitsTool {...props} />);
    const quick = screen.getByLabelText('Quick conversion');
    await user.type(quick, '2 lb to kg');
    expect(await screen.findByText(/2 Pound = 0.90718474 Kilogram/)).toBeInTheDocument();
    await user.keyboard('{Enter}');
    expect(await screen.findByText('0.90718474')).toBeInTheDocument();
    expect((screen.getByLabelText('Category') as HTMLSelectElement).value).toBe('mass');
    await user.type(quick, '5 km to kg');
    expect(await screen.findByText('These units measure different things.')).toBeInTheDocument();
  });

  it('lists the value in every unit of the category', async () => {
    const user = userEvent.setup();
    render(<UnitsTool {...props} />);
    await user.click(screen.getByRole('checkbox', { name: 'Show every unit of this category' }));
    const table = await screen.findByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(10);
    expect(within(table).getByRole('row', { name: /Mile/ })).toHaveTextContent('0.621371192237');
  });

  it('names every unit the engine knows', async () => {
    const engine = await getToolsEngine();
    const ids = engine.unitsList().categories.flatMap((c) => c.units.map((u) => u.id));
    expect(ids.length).toBeGreaterThan(70);
    for (const id of ids) {
      const key = UNIT_NAME_KEYS[id];
      expect(key, id).toBeTruthy();
      expect(en[key!], id).toBeTruthy();
    }
    expect(Object.keys(UNIT_NAME_KEYS).sort()).toEqual([...ids].sort());
  });

  it('is accessible', async () => {
    const { container } = render(<UnitsTool {...props} />);
    await screen.findByText('0.621371192237');
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe.skipIf(!built)('percent', () => {
  it('reads a phrase', async () => {
    const user = userEvent.setup();
    render(<PercentTool {...props} />);
    await user.type(screen.getByLabelText('Phrase (in English)'), '15% of 80');
    expect(await screen.findByText('15% of 80 is 12')).toBeInTheDocument();
  });

  it('offers examples that work', async () => {
    const user = userEvent.setup();
    render(<PercentTool {...props} />);
    await user.click(screen.getByRole('button', { name: '12 is what % of 60' }));
    expect(await screen.findByText('12 is 20% of 60')).toBeInTheDocument();
  });

  it('says when it does not understand', async () => {
    const user = userEvent.setup();
    render(<PercentTool {...props} />);
    await user.type(screen.getByLabelText('Phrase (in English)'), 'banana');
    expect(await screen.findByText(/This phrase is not recognised/)).toBeInTheDocument();
  });

  it('works from the form too', async () => {
    const user = userEvent.setup();
    render(<PercentTool {...props} />);
    await user.selectOptions(screen.getByLabelText('Calculation'), 'add');
    await user.type(screen.getByLabelText('Amount'), '80');
    await user.type(screen.getByLabelText('Percent to add'), '15');
    expect(await screen.findByText('80 plus 15% is 92')).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText('Calculation'), 'change');
    expect(screen.getByLabelText('From')).toHaveValue('80');
    await user.clear(screen.getByLabelText('From'));
    await user.type(screen.getByLabelText('From'), '0');
    await user.clear(screen.getByLabelText('To'));
    await user.type(screen.getByLabelText('To'), '5');
    expect(await screen.findByText('Division by zero.')).toBeInTheDocument();
  });

  it('is accessible', async () => {
    const { container } = render(<PercentTool {...props} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('every error code has words', () => {
  it('maps each code the Rust modules can answer with to an English message', () => {
    const codes = [
      'empty',
      'unexpected_char',
      'unexpected_token',
      'unexpected_end',
      'unbalanced',
      'divide_by_zero',
      'domain',
      'overflow',
      'bad_number',
      'unknown_name',
      'unknown_function',
      'missing_argument',
      'arity',
      'no_answer',
      'too_deep',
      'too_many_arguments',
      'bad_digit',
      'literal_too_large',
      'shift_range',
      'bad_width',
      'bad_base',
      'category_mismatch',
      'unknown_unit',
      'no_target',
      'below_absolute_zero',
      'no_phrase',
    ];
    expect([...ERROR_CODES].sort()).toEqual([...codes].sort());
    for (const c of codes) expect(en[errorKey(c)], c).toBeTruthy();
    expect(en[errorKey('something_new')]).toBeTruthy();
  });
});

describe('currency', () => {
  const rates = {
    enabled: true,
    base: 'EUR',
    as_of: '2026-10-29',
    fetched_at: 1_790_000_000,
    stale: false,
    rates: { USD: 1.08, GBP: 0.84, JPY: 160 },
  };
  const serve = (body: unknown, status = 200) =>
    server.use(http.get('*/api/v1/tools/fx', () => HttpResponse.json(body as object, { status })));

  it('converts through the euro and says where the rates come from', async () => {
    serve(rates);
    const user = userEvent.setup();
    render(<CurrencyTool {...props} />);
    const amount = await screen.findByLabelText('Amount');
    expect(await screen.findByText('$108.00')).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText('From'), 'USD');
    await user.selectOptions(screen.getByLabelText('To'), 'GBP');
    await user.clear(amount);
    await user.type(amount, '216');
    expect(await screen.findByText('£168.00')).toBeInTheDocument();
    expect(screen.getByText(/European Central Bank/)).toBeInTheDocument();
    expect(screen.queryByText(/out of date/)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Swap the currencies' }));
    expect(await screen.findByText('$277.71')).toBeInTheDocument();
  });

  it('warns when the rates are stale', async () => {
    serve({ ...rates, stale: true });
    render(<CurrencyTool {...props} />);
    expect(await screen.findByText(/These rates are out of date/)).toBeInTheDocument();
  });

  it('is hidden when the server switched it off', async () => {
    serve({ enabled: false, base: 'EUR', as_of: null, fetched_at: null, stale: false, rates: {} });
    render(<CurrencyTool {...props} />);
    expect(
      await screen.findByText('The currency converter is switched off on this server.'),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText('Amount')).not.toBeInTheDocument();
  });

  it('says so before the first fetch and when the server fails', async () => {
    serve({ enabled: true, base: 'EUR', as_of: null, fetched_at: null, stale: true, rates: {} });
    const first = render(<CurrencyTool {...props} />);
    expect(await screen.findByText(/No exchange rates have been fetched yet/)).toBeInTheDocument();
    first.unmount();
    serve({ message: 'boom' }, 500);
    render(<CurrencyTool {...props} />);
    expect(await screen.findByRole('alert')).toHaveTextContent(/could not be loaded/);
  });

  it('refuses an amount that is not a number and survives a stale saved pair', async () => {
    serve(rates);
    localStorage.setItem('sr.tools.currency.pair', JSON.stringify(['ZZZ', 'QQQ']));
    const user = userEvent.setup();
    render(<CurrencyTool {...props} />);
    const amount = await screen.findByLabelText('Amount');
    expect((screen.getByLabelText('From') as HTMLSelectElement).value).toBe('EUR');
    await user.clear(amount);
    await user.type(amount, 'abc');
    expect(await screen.findByText('This amount is not a number.')).toBeInTheDocument();
  });

  it('is accessible', async () => {
    serve(rates);
    const { container } = render(<CurrencyTool {...props} />);
    await screen.findByLabelText('Amount');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('never talks to anyone but the app server', async () => {
    serve(rates);
    const seen: string[] = [];
    const spy = vi.spyOn(globalThis, 'fetch');
    render(<CurrencyTool {...props} />);
    await screen.findByLabelText('Amount');
    for (const call of spy.mock.calls) {
      const input = call[0];
      seen.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url);
    }
    spy.mockRestore();
    expect(seen.length).toBeGreaterThan(0);
    expect(seen.every((u) => /\/api\/v1\/tools\/fx$/.test(u))).toBe(true);
  });
});
