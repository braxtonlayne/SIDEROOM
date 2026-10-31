// @vitest-environment jsdom
// The UI on a loopback room with a host and a guest, in jsdom (canvas calls stubbed): the intents the buttons
// send, and what each seat shows. Run from apps/web (it has jsdom): see the handoff.
import { startApp } from '../src/app.js';
import { Fog } from '../src/fog.js';
import { handleInput, K } from '../src/table.js';
import { createLoopbackRoom, runHost } from '../src/transport.js';

const noop = () => undefined;
const ctx = new Proxy({}, { get: (_, k) => (k === 'measureText' ? () => ({ width: 10 }) : noop), set: () => true });

beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = () => ctx;
  globalThis.ResizeObserver = class {
    observe() {}
    disconnect() {}
  };
  globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 0);
  HTMLElement.prototype.setPointerCapture = noop;
  // A 500 x 400 stage: the 500 x 400 map fits at scale 1, so a cell (c, r) of 50 px is at (50c.., 50r..).
  HTMLElement.prototype.getBoundingClientRect = () => ({ left: 0, top: 0, right: 500, bottom: 400, width: 500, height: 400, x: 0, y: 0 });
});

const MAP = { kind: 'square', size: 50, ox: 0, oy: 0, width: 500, height: 400, unit: 5, unitName: 'ft' };

function seats() {
  const room = createLoopbackRoom('host');
  const host = room.join({ id: 'host', name: 'Hosta' });
  const guest = room.join({ id: 'guest', name: 'Gus' });
  const hooks = {};
  runHost(host, handleInput, { hooks, seed: () => 0xabcn });
  const a = document.createElement('div');
  const b = document.createElement('div');
  document.body.append(a, b);
  startApp(a, host, { hooks });
  startApp(b, guest, {});
  host.set(K.map, MAP);
  return { room, host, guest, a, b };
}
const click = (root, sel) => root.querySelector(sel).click();

describe('tabletop UI', () => {
  it('shows the host controls only to the host', () => {
    const { a, b } = seats();
    expect(a.querySelector('[data-testid=role]').textContent).toMatch(/host/i);
    expect(a.querySelector('[data-testid=map-apply]')).not.toBeNull();
    expect(b.querySelector('[data-testid=map-apply]')).toBeNull();
    expect(b.querySelector('[data-tool=hide]')).toBeNull();
    expect(a.querySelector('[data-tool=hide]')).not.toBeNull();
  });
  it('a guest adds a token and both seats list it', async () => {
    const { a, b, room } = seats();
    click(b, '[data-testid=add-mine]');
    await new Promise((r) => setTimeout(r, 20));
    expect(Object.keys(room.doc).filter((k) => k.startsWith('host:tok:'))).toHaveLength(1);
    expect(a.querySelectorAll('.tokrow')).toHaveLength(1);
    expect(b.querySelector('[data-testid=add-mine]')).toBeNull(); // one token each
  });
  it('rolls dice from the tray; both seats show the total, the seed and a passing check', () => {
    const { a, b } = seats();
    const expr = b.querySelector('[data-testid=dice-expr]');
    expr.value = '2d20kh1+5';
    click(b, '[data-testid=dice-roll]');
    for (const seat of [a, b]) {
      expect(seat.querySelector('[data-testid=roll-seed]').textContent).toBe('0000000000000abc');
      expect(Number(seat.querySelector('[data-testid=roll-total]').textContent)).toBeGreaterThanOrEqual(6);
      expect(seat.querySelector('.roll .good')).not.toBeNull();
      expect(seat.querySelector('.roll .who').textContent).toContain('Gus');
    }
  });
  it('refuses a bad expression locally with its code and sends nothing', () => {
    const { b, room } = seats();
    b.querySelector('[data-testid=dice-expr]').value = '2d';
    click(b, '[data-testid=dice-roll]');
    expect(b.querySelector('.error').textContent).toMatch(/syntax/);
    expect(room.doc[K.rollSeq]).toBeUndefined();
  });
  it('die buttons build the expression', () => {
    const { b } = seats();
    const expr = b.querySelector('[data-testid=dice-expr]');
    click(b, '[data-die="20"]');
    click(b, '[data-die="20"]');
    expect(expr.value).toBe('2d20');
    click(b, '[data-die="6"]');
    expect(expr.value).toBe('2d20+1d6');
  });
  it('the host starts initiative and the round shows on both seats', async () => {
    const { a, b, host } = seats();
    host.send({ t: 'init', op: 'add', entry: { id: 'x', name: 'Xan', init: 10 } });
    click(a, '[data-testid=init-start]');
    expect(a.querySelector('[data-testid=round]').textContent).toMatch(/Round 1/);
    expect(b.querySelector('[data-testid=round]').textContent).toMatch(/Xan/);
    expect(b.querySelector('[data-testid=init-start]')).toBeNull();
  });
});

describe('board gestures', () => {
  const at = (c, r) => ({ clientX: c * 50 + 25, clientY: r * 50 + 25 });
  const fire = (el, type, pos, init = {}) =>
    el.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0, ...pos, ...init }));
  const board = (root) => root.querySelector('canvas');
  const tick = () => new Promise((r) => setTimeout(r, 10));

  it('drags a token to another cell in one context and the other sees it', async () => {
    const { b, a, host, room } = seats();
    click(b, '[data-testid=add-mine]');
    await tick();
    const id = Object.keys(room.doc).find((k) => k.startsWith('host:tok:'));
    const start = room.doc[id];
    expect([start.c, start.r]).toEqual([0, 0]);
    const cv = board(b);
    fire(cv, 'pointerdown', at(0, 0));
    fire(cv, 'pointermove', at(3, 2));
    fire(cv, 'pointerup', at(3, 2));
    expect(room.doc[id]).toMatchObject({ c: 3, r: 2 });
    expect(host.doc()[id]).toMatchObject({ c: 3, r: 2 });
    expect(a.querySelectorAll('.tokrow')).toHaveLength(1);
  });
  it('a guest cannot drag a token that is not theirs', async () => {
    const { b, host, room } = seats();
    host.send({ t: 'token-add', token: { id: 'orc', name: 'Orc', c: 4, r: 4 } });
    const cv = board(b);
    fire(cv, 'pointerdown', at(4, 4));
    fire(cv, 'pointermove', at(6, 6));
    fire(cv, 'pointerup', at(6, 6));
    expect(room.doc['host:tok:orc']).toMatchObject({ c: 4, r: 4 });
  });
  it('measures between two cells and publishes the ruler for others', () => {
    const { a, b, room } = seats();
    click(b, '[data-tool=measure]');
    const cv = board(b);
    fire(cv, 'pointerdown', at(1, 1));
    fire(cv, 'pointermove', at(5, 4));
    fire(cv, 'pointerup', at(5, 4));
    expect(b.querySelector('.status').textContent).toBe('20 ft (4 cells)');
    expect(room.doc['ruler:guest']).toEqual({ a: { c: 1, r: 1 }, b: { c: 5, r: 4 } });
    expect(a.querySelector('.status').textContent).toBe('');
  });
  it('the host paints fog with the brush and a rectangle, and the guest receives it', () => {
    const { a, room } = seats();
    click(a, '[data-tool=hide]');
    const cv = board(a);
    fire(cv, 'pointerdown', at(5, 4));
    fire(cv, 'pointerup', at(5, 4));
    const one = Fog.decode(room.doc['host:fog']);
    expect(one.isHidden(5, 4)).toBe(true);
    expect(one.isHidden(6, 4)).toBe(true); // radius 1 round brush
    expect(one.isHidden(0, 0)).toBe(false);
    a.querySelector('select[aria-label]').value = 'rect';
    a.querySelector('select[aria-label]').dispatchEvent(new Event('change', { bubbles: true }));
    fire(cv, 'pointerdown', at(0, 0));
    fire(cv, 'pointermove', at(1, 1));
    fire(cv, 'pointerup', at(1, 1));
    const two = Fog.decode(room.doc['host:fog']);
    expect([two.isHidden(0, 0), two.isHidden(1, 1), two.isHidden(2, 2), two.isHidden(5, 4)]).toEqual([true, true, false, true]);
    click(a, '[data-tool=reveal]');
    fire(cv, 'pointerdown', at(0, 0));
    fire(cv, 'pointerup', at(0, 0));
    expect(Fog.decode(room.doc['host:fog']).isHidden(0, 0)).toBe(false);
  });
  it('a guest has no fog tools and fog does not move through the guest', () => {
    const { b, room } = seats();
    const cv = board(b);
    fire(cv, 'pointerdown', at(5, 4));
    fire(cv, 'pointerup', at(5, 4));
    expect(room.doc['host:fog']).toBeUndefined();
  });
});
