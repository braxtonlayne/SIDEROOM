import {
  AXIS_THRESHOLD,
  assignPlayers,
  captureSkip,
  captureStep,
  CAPTURE_ORDER,
  defaultMapping,
  diffButtons,
  firstBinding,
  loadMapping,
  MAPPING_KEY,
  parseMapping,
  readKeyboard,
  readPad,
  retroIndex,
  saveMapping,
  startCapture,
  type PadLike,
} from '../gamepad';
import { InputPoller } from '../input';

const pad = (over: Partial<PadLike> = {}): PadLike => ({
  id: 'Test Pad',
  index: 0,
  connected: true,
  buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })),
  axes: [0, 0, 0, 0],
  ...over,
});
const press = (p: PadLike, ...idx: number[]): PadLike => ({
  ...p,
  buttons: p.buttons.map((b, i) => (idx.includes(i) ? { pressed: true, value: 1 } : b)),
});

describe('RetroPad indices', () => {
  it('follow RetroArch joypad order', () => {
    expect(retroIndex('b')).toBe(0);
    expect(retroIndex('start')).toBe(3);
    expect(retroIndex('up')).toBe(4);
    expect(retroIndex('a')).toBe(8);
    expect(retroIndex('r3')).toBe(15);
  });
});

describe('readPad', () => {
  it('maps the standard layout: south is B, east is A, the d-pad is buttons 12 to 15', () => {
    const down = readPad(press(pad(), 0, 1, 9, 12), defaultMapping());
    expect([...down].sort()).toEqual(['a', 'b', 'start', 'up']);
  });
  it('lets the left stick drive the d-pad while the d-pad is idle', () => {
    const left = readPad(pad({ axes: [-1, 0, 0, 0] }), defaultMapping());
    expect([...left]).toEqual(['left']);
    const both = readPad(press(pad({ axes: [-1, 0, 0, 0] }), 15), defaultMapping());
    expect([...both]).toEqual(['right']);
  });
  it('ignores a stick inside the dead zone', () => {
    const still = pad({ axes: [AXIS_THRESHOLD, -AXIS_THRESHOLD, 0, 0] });
    expect(readPad(still, defaultMapping()).size).toBe(0);
  });
  it('reads a custom axis binding', () => {
    const m = defaultMapping();
    m.gamepad.a = { kind: 'axis', index: 2, dir: 1 };
    expect(readPad(pad({ axes: [0, 0, 0.9, 0] }), m).has('a')).toBe(true);
    expect(readPad(pad({ axes: [0, 0, -0.9, 0] }), m).has('a')).toBe(false);
  });
});

describe('readKeyboard and diffButtons', () => {
  it('reads held key codes through the mapping', () => {
    const held = new Set(['KeyZ', 'ArrowUp', 'KeyK']);
    expect([...readKeyboard(held, defaultMapping())].sort()).toEqual(['b', 'up']);
  });
  it('reports only changes', () => {
    const ev = diffButtons(1, new Set(['a', 'b'] as const), new Set(['b', 'x'] as const));
    expect(ev).toEqual([
      { player: 1, button: 'a', pressed: false },
      { player: 1, button: 'x', pressed: true },
    ]);
  });
});

describe('players', () => {
  it('takes connected pads in port order, at most four', () => {
    const pads = [4, 3, 2, 1, 0].map((i) => pad({ index: i }));
    pads.push(pad({ index: 9, connected: false }));
    expect(assignPlayers([...pads, null]).map((p) => p.index)).toEqual([0, 1, 2, 3]);
  });
  it('sends a keyboard press to player 1 and a second pad to player 2, once', () => {
    const sent: string[] = [];
    let pads: PadLike[] = [pad({ index: 0 }), pad({ index: 1 })];
    const poller = new InputPoller(
      (p, b, v) => sent.push(`${p}:${b}:${v ? 1 : 0}`),
      () => defaultMapping(),
      () => pads,
    );
    poller.key('KeyZ', true);
    pads = [pad({ index: 0 }), press(pad({ index: 1 }), 1)];
    poller.poll();
    poller.poll();
    expect(sent).toEqual(['0:b:1', '1:a:1']);
    poller.key('KeyZ', false);
    poller.poll();
    expect(sent.at(-1)).toBe('0:b:0');
    poller.releaseAll();
    expect(sent.at(-1)).toBe('1:a:0');
  });
});

describe('the mapping screen', () => {
  it('walks the buttons in order and takes a control away from the button that had it', () => {
    let s = startCapture('keyboard', defaultMapping());
    expect(s.current).toBe(CAPTURE_ORDER[0]);
    s = captureStep(s, { key: 'KeyZ' }); // up takes Z, which was B's
    expect(s.mapping.keyboard.up).toBe('KeyZ');
    expect(s.mapping.keyboard.b).toBeUndefined();
    expect(s.current).toBe('down');
    s = captureSkip(s);
    expect(s.current).toBe('left');
    expect(s.mapping.keyboard.down).toBe('ArrowDown');
  });
  it('ignores the wrong kind of input and ends after the last button', () => {
    let s = startCapture('gamepad', defaultMapping());
    s = captureStep(s, { key: 'KeyZ' });
    expect(s.current).toBe(CAPTURE_ORDER[0]);
    for (let i = 0; i < CAPTURE_ORDER.length; i++) {
      s = captureStep(s, { binding: { kind: 'button', index: 20 + i } });
    }
    expect(s.current).toBeNull();
    expect(s.mapping.gamepad.select).toEqual({ kind: 'button', index: 31 });
    expect(captureStep(s, { binding: { kind: 'button', index: 1 } })).toBe(s);
  });
  it('finds the control being pressed', () => {
    expect(firstBinding(pad())).toBeNull();
    expect(firstBinding(press(pad(), 5))).toEqual({ kind: 'button', index: 5 });
    expect(firstBinding(pad({ axes: [0, 0, 0, -0.95] }))).toEqual({
      kind: 'axis',
      index: 3,
      dir: -1,
    });
  });
});

describe('the saved mapping', () => {
  it('round-trips and survives garbage', () => {
    const m = defaultMapping();
    m.keyboard.a = 'KeyJ';
    m.gamepad.a = { kind: 'axis', index: 3, dir: -1 };
    const mem = new Map<string, string>();
    const store = {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => void mem.set(k, v),
    };
    saveMapping(store, m);
    expect(mem.has(MAPPING_KEY)).toBe(true);
    expect(loadMapping(store)).toEqual(m);
    expect(parseMapping('{not json')).toEqual(defaultMapping());
    const bad = parseMapping(
      JSON.stringify({
        keyboard: { a: 5, b: 'KeyP' },
        gamepad: { a: { kind: 'button', index: -1 }, b: { kind: 'x' } },
      }),
    );
    expect(bad.keyboard.a).toBe('KeyX');
    expect(bad.keyboard.b).toBe('KeyP');
    expect(bad.gamepad.a).toEqual(defaultMapping().gamepad.a);
  });
  it('lives with a blocked storage', () => {
    const blocked = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(loadMapping(blocked)).toEqual(defaultMapping());
    expect(() => saveMapping(blocked, defaultMapping())).not.toThrow();
  });
});
