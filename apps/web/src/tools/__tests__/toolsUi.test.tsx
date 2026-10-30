import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { en } from '../../i18n/messages/en';
import { TOOLS, getTool, registryProblems } from '../registry';
import { type DevWasmApi, setDevWasmForTests } from '../dev/wasmTools';
import Base64Tool from '../dev/Base64Tool';
import CounterTool from '../dev/CounterTool';
import DiffTool from '../dev/DiffTool';
import HashTool from '../dev/HashTool';
import JsonTool from '../dev/JsonTool';
import JwtTool from '../dev/JwtTool';
import MdTableTool from '../dev/MdTableTool';
import RegexTool from '../dev/RegexTool';
import UrlTool from '../dev/UrlTool';
import CoinTool from '../dice/CoinTool';
import DiceTool from '../dice/DiceTool';
import WheelTool, { prefersReducedMotion } from '../dice/WheelTool';
import PomodoroTool from '../timers/PomodoroTool';
import StopwatchTool from '../timers/StopwatchTool';
import CountdownTool from '../timers/CountdownTool';
import { resetSharedTimers } from '../timers/timerStore';

const props = { host: 'drawer' as const };

const wasm: DevWasmApi = {
  diceRoll: (expr) => {
    if (expr === 'bad') throw new Error('syntax:1');
    return JSON.stringify({
      expression: expr,
      total: 13,
      breakdown: '13',
      groups: [
        {
          notation: '2d6',
          total: 13,
          dice: [
            { value: 6, dropped: false, exploded: false, success: false },
            { value: 7, dropped: true, exploded: false, success: false },
          ],
        },
      ],
    });
  },
  diceFlip: () => true,
  dicePickWeighted: (w) => w.length - 1,
  diffText: () =>
    JSON.stringify({
      ops: [
        { op: 'eq', text: 'same\n' },
        { op: 'del', text: 'old\n' },
        { op: 'add', text: 'new\n' },
      ],
      added: 1,
      removed: 1,
    }),
  hashMd5: () => 'd41d8cd98f00b204e9800998ecf8427e',
  hashCrc32: () => '00000000',
};

beforeEach(() => setDevWasmForTests(async () => wasm));
afterEach(() => {
  setDevWasmForTests(null);
  resetSharedTimers();
  try {
    localStorage.clear();
  } catch {
    /* fine */
  }
  vi.useRealTimers();
});

const type = (label: string | RegExp, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });

describe('the registry entries', () => {
  const ids = [
    'json',
    'regex',
    'base64',
    'url',
    'jwt',
    'md-table',
    'counter',
    'hash',
    'diff',
    'dice',
    'coin',
    'wheel',
    'loot',
    'notes',
    'pomodoro',
    'stopwatch',
    'countdown',
  ];

  it('are all there, well formed and unique', () => {
    expect(registryProblems()).toEqual([]);
    for (const id of ids) expect(getTool(id), id).toBeDefined();
  });

  it('have a name and a description in the catalog', () => {
    for (const id of ids) {
      const e = getTool(id)!;
      expect(en[e.nameKey], e.nameKey).toBeTruthy();
      expect(en[e.descriptionKey], e.descriptionKey).toBeTruthy();
    }
  });

  it('load a component that renders in the drawer', async () => {
    const simple = ids.filter((id) => !['loot', 'notes'].includes(id));
    for (const id of simple) {
      const mod = await getTool(id)!.load();
      const { unmount, container } = render(<mod.default host="drawer" />);
      expect(container.firstChild, id).not.toBeNull();
      unmount();
    }
    expect(TOOLS.length).toBeGreaterThanOrEqual(ids.length);
  });
});

describe('developer tools', () => {
  it('JSON: formats valid input and names the line of a mistake', () => {
    render(<JsonTool />);
    type('JSON to check', '{"a":1,"b":[1,2]}');
    expect(screen.getByRole('status').textContent).toMatch(/valid JSON/);
    expect(screen.getByLabelText('Result').textContent).toContain('"a": 1');
    type('JSON to check', '{"a":}');
    expect(screen.getByRole('alert')).toBeTruthy();
  });

  it('Base64 and URL encode and decode', () => {
    const { unmount } = render(<Base64Tool />);
    type('Text', 'hello');
    expect(screen.getByLabelText('Result').textContent).toBe('aGVsbG8=');
    fireEvent.click(screen.getByRole('button', { name: 'Decode' }));
    type('Text', '!!!');
    expect(screen.getByRole('alert')).toBeTruthy();
    unmount();

    render(<UrlTool />);
    type('Text or address', 'a b&c');
    expect(screen.getByLabelText('Result').textContent).toBe('a%20b%26c');
  });

  it('JWT: always says it does not verify, and reads a token', () => {
    render(<JwtTool />);
    expect(screen.getByRole('note').textContent).toMatch(/does not check the signature/);
    const seg = (o: unknown) => btoa(JSON.stringify(o)).replace(/=+$/, '');
    type('Token', `${seg({ alg: 'none' })}.${seg({ sub: 'me', exp: 1_700_000_000 })}.sig`);
    expect(screen.getByText('Payload')).toBeTruthy();
    expect(screen.getByText(/2023-11-14/)).toBeTruthy();
    expect(screen.getByText('Signature not verified.')).toBeTruthy();
  });

  it('regex: shows matches and a bad pattern', () => {
    render(<RegexTool />);
    expect(screen.getByText(/JavaScript regular expressions/)).toBeTruthy();
    type('Pattern', '\\d+');
    type('Text to search', 'a1 b22');
    expect(screen.getByText('2 matches')).toBeTruthy();
    type('Pattern', '(');
    expect(screen.getByRole('alert')).toBeTruthy();
  });

  it('Markdown table and counter', () => {
    const { unmount } = render(<MdTableTool />);
    type(/Rows/, 'a,b\n1,2');
    expect(screen.getByLabelText('Markdown').textContent).toContain('| a   | b   |');
    unmount();
    render(<CounterTool />);
    type('Text', 'one two three');
    expect(screen.getByText('Words').nextSibling?.textContent).toBe('3');
  });

  it('hashes show MD5, CRC-32 and the SHA family', async () => {
    render(<HashTool />);
    type('Text', '');
    await waitFor(() => expect(screen.getByText('MD5')).toBeTruthy());
    expect(screen.getByText('SHA-256')).toBeTruthy();
    expect(screen.getByText(/e3b0c44298fc1c149afbf4c8996fb924/)).toBeTruthy();
    expect(screen.getByText('CRC-32')).toBeTruthy();
  });

  it('diff shows additions and removals from the wasm', async () => {
    render(<DiffTool />);
    type('Before', 'same\nold\n');
    type('After', 'same\nnew\n');
    await waitFor(() => expect(screen.getByText('1 added, 1 removed')).toBeTruthy());
    expect(screen.getByLabelText('Differences').querySelector('ins')?.textContent).toBe('new\n');
    expect(screen.getByLabelText('Differences').querySelector('del')?.textContent).toBe('old\n');
    type('Compare by', 'json');
    type('Before', '{bad');
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/not valid JSON/));
  });
});

describe('dice tools', () => {
  it('rolls, shows dropped dice, and explains a bad expression', async () => {
    render(<DiceTool />);
    fireEvent.click(screen.getByRole('button', { name: 'Roll' }));
    await waitFor(() => expect(screen.getByText('13')).toBeTruthy());
    expect(screen.getByText('(dropped)')).toBeTruthy();
    type('Dice notation', 'bad');
    fireEvent.click(screen.getByRole('button', { name: 'Roll' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/not dice notation/));
  });

  it('coin flips and keeps a tally', async () => {
    render(<CoinTool />);
    fireEvent.click(screen.getByRole('button', { name: 'Flip' }));
    await waitFor(() => expect(screen.getByText('Heads 1, tails 0')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
    expect(screen.getByText('Heads 0, tails 0')).toBeTruthy();
  });

  it('the wheel announces the pick; with reduced motion it does not spin', async () => {
    document.documentElement.dataset.reducedMotion = 'true';
    try {
      expect(prefersReducedMotion()).toBe(true);
      render(<WheelTool />);
      expect(screen.getByRole('button', { name: 'Spin' }).hasAttribute('disabled')).toBe(true);
      type('Names, one per line', 'Alex\nSam\nJo\nAlex');
      expect(screen.getByText('3 names')).toBeTruthy();
      fireEvent.click(screen.getByRole('button', { name: 'Spin' }));
      await waitFor(() => expect(screen.getByText('It’s Jo!')).toBeTruthy());
      const face = document.querySelector<HTMLElement>('.ud-wheel-face')!;
      // Straight to the answer: less than one full turn.
      const deg = Number(/rotate\(([-\d.]+)deg\)/.exec(face.style.transform)![1]);
      expect(Math.abs(deg)).toBeLessThan(360);
    } finally {
      delete document.documentElement.dataset.reducedMotion;
    }
  });

  it('the wheel turns several times when motion is allowed', async () => {
    render(<WheelTool />);
    type('Names, one per line', 'A\nB');
    fireEvent.click(screen.getByRole('button', { name: 'Spin' }));
    await waitFor(() => expect(screen.getByText('It’s B!')).toBeTruthy());
    const deg = Number(
      /rotate\(([-\d.]+)deg\)/.exec(
        document.querySelector<HTMLElement>('.ud-wheel-face')!.style.transform,
      )![1],
    );
    expect(deg).toBeGreaterThan(1440);
    expect(deg % 360).toBeCloseTo(90, 5); // B is the second of two: its middle (270 degrees) lands at the top
  });
});

describe('timers', () => {
  it('a pomodoro runs, pauses and resets from the buttons', async () => {
    vi.useFakeTimers({
      toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'],
    });
    vi.setSystemTime(new Date('2030-01-01T00:00:00Z'));
    render(<PomodoroTool {...props} />);
    const clock = () => screen.getByRole('timer').textContent;
    expect(clock()).toBe('25:00');
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));
    await act(async () => {
      vi.advanceTimersByTime(65_000);
    });
    expect(clock()).toBe('23:55');
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    await act(async () => {
      vi.advanceTimersByTime(60_000);
    });
    expect(clock()).toBe('23:55');
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
    expect(clock()).toBe('25:00');
  });

  it('a countdown takes a typed length and announces the end', async () => {
    vi.useFakeTimers({
      toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'],
    });
    vi.setSystemTime(new Date('2030-01-01T00:00:00Z'));
    render(<CountdownTool {...props} />);
    type(/Length/, '10s');
    fireEvent.click(screen.getByRole('button', { name: 'Set' }));
    expect(screen.getByRole('timer').textContent).toBe('00:10');
    type(/Length/, 'soon');
    fireEvent.click(screen.getByRole('button', { name: 'Set' }));
    expect(screen.getByRole('alert')).toBeTruthy();
    type(/Length/, '10s');
    fireEvent.click(screen.getByRole('button', { name: 'Set' }));
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));
    await act(async () => {
      vi.advanceTimersByTime(11_000);
    });
    expect(screen.getByRole('timer').textContent).toBe('00:00');
    expect(screen.getByText('Time is up.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Start again' })).toBeTruthy();
  });

  it('a stopwatch counts up and keeps laps', async () => {
    vi.useFakeTimers({
      toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'],
    });
    vi.setSystemTime(new Date('2030-01-01T00:00:00Z'));
    render(<StopwatchTool {...props} />);
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    fireEvent.click(screen.getByRole('button', { name: 'Lap' }));
    expect(screen.getByRole('list', { name: 'Laps' }).children).toHaveLength(1);
    expect(screen.getByRole('timer').textContent).toMatch(/^00:0[12]\.\d$/);
  });

  it('without a voice channel the share control says how to use it', () => {
    render(<PomodoroTool {...props} />);
    expect(screen.getByText('Join a voice channel to share this timer with it.')).toBeTruthy();
  });
});

describe('accessibility', () => {
  it('has no axe violations in the tools', async () => {
    const tools = [
      JsonTool,
      RegexTool,
      Base64Tool,
      UrlTool,
      JwtTool,
      MdTableTool,
      CounterTool,
      DiceTool,
      CoinTool,
      WheelTool,
      PomodoroTool,
      StopwatchTool,
      CountdownTool,
      DiffTool,
      HashTool,
    ];
    for (const Tool of tools) {
      const { container, unmount } = render(<Tool {...props} />);
      expect(await axeViolations(container), Tool.name).toEqual([]);
      unmount();
    }
  });
});
