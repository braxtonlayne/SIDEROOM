import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PttController, ShortcutListener, suspendKeybinds } from '../ptt';

function key(
  type: 'keydown' | 'keyup',
  code: string,
  init: KeyboardEventInit = {},
  target: EventTarget = document.body,
) {
  const e = new KeyboardEvent(type, { code, key: code, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
}

let ptt: PttController;
let changes: boolean[];

beforeEach(() => {
  vi.useFakeTimers();
  changes = [];
  ptt = new PttController((a) => changes.push(a));
  ptt.attach();
});
afterEach(() => {
  ptt.detach();
  vi.useRealTimers();
});

describe('push to talk', () => {
  it('opens on keydown and closes after the release delay', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 500, enabled: true });
    key('keydown', 'KeyV');
    expect(ptt.isActive).toBe(true);
    key('keyup', 'KeyV');
    // Still open during the delay.
    vi.advanceTimersByTime(499);
    expect(ptt.isActive).toBe(true);
    vi.advanceTimersByTime(2);
    expect(ptt.isActive).toBe(false);
    expect(changes).toEqual([true, false]);
  });

  it('closes at once with a zero release delay', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 0, enabled: true });
    key('keydown', 'KeyV');
    key('keyup', 'KeyV');
    expect(ptt.isActive).toBe(false);
  });

  it('pressing again inside the delay keeps it open', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 300, enabled: true });
    key('keydown', 'KeyV');
    key('keyup', 'KeyV');
    vi.advanceTimersByTime(200);
    key('keydown', 'KeyV');
    vi.advanceTimersByTime(1000);
    expect(ptt.isActive).toBe(true);
    expect(changes).toEqual([true]);
  });

  it('honours the maximum 2000 ms delay', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 2000, enabled: true });
    key('keydown', 'KeyV');
    key('keyup', 'KeyV');
    vi.advanceTimersByTime(1999);
    expect(ptt.isActive).toBe(true);
    vi.advanceTimersByTime(2);
    expect(ptt.isActive).toBe(false);
  });

  it('never sends the PTT key to the composer', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 0, enabled: true });
    const composer = document.createElement('textarea');
    document.body.append(composer);
    const seen: string[] = [];
    composer.addEventListener('keydown', () => seen.push('keydown'));
    composer.addEventListener('keyup', () => seen.push('keyup'));
    const down = key('keydown', 'KeyV', {}, composer);
    const up = key('keyup', 'KeyV', {}, composer);
    expect(seen).toEqual([]);
    expect(down.defaultPrevented).toBe(true);
    expect(up.defaultPrevented).toBe(true);
    // Other keys still reach it.
    key('keydown', 'KeyA', {}, composer);
    expect(seen).toEqual(['keydown']);
    composer.remove();
  });

  it('leaves the key alone when PTT is not in effect', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 0, enabled: false });
    const e = key('keydown', 'KeyV');
    expect(e.defaultPrevented).toBe(false);
    expect(ptt.isActive).toBe(false);
  });

  it('stops transmitting on blur and when disabled mid-press', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 1000, enabled: true });
    key('keydown', 'KeyV');
    window.dispatchEvent(new Event('blur'));
    expect(ptt.isActive).toBe(false);
    key('keydown', 'KeyV');
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 1000, enabled: false });
    expect(ptt.isActive).toBe(false);
  });

  it('supports chords and ends when a required modifier is released', () => {
    ptt.configure({ bind: { code: 'KeyV', ctrl: true }, releaseMs: 0, enabled: true });
    key('keydown', 'KeyV');
    expect(ptt.isActive).toBe(false);
    key('keydown', 'KeyV', { ctrlKey: true });
    expect(ptt.isActive).toBe(true);
    key('keyup', 'ControlLeft');
    expect(ptt.isActive).toBe(false);
  });

  it('is suspended while a keybind is being recorded', () => {
    ptt.configure({ bind: { code: 'KeyV' }, releaseMs: 0, enabled: true });
    const resume = suspendKeybinds();
    const e = key('keydown', 'KeyV');
    expect(e.defaultPrevented).toBe(false);
    expect(ptt.isActive).toBe(false);
    resume();
    key('keydown', 'KeyV');
    expect(ptt.isActive).toBe(true);
  });
});

describe('shortcut listener', () => {
  it('fires once per press and only while enabled', () => {
    const onMute = vi.fn();
    const l = new ShortcutListener();
    l.attach();
    l.configure({ toggleMute: { code: 'KeyM', ctrl: true } }, { toggleMute: onMute }, false);
    key('keydown', 'KeyM', { ctrlKey: true });
    expect(onMute).not.toHaveBeenCalled();
    l.configure({ toggleMute: { code: 'KeyM', ctrl: true } }, { toggleMute: onMute }, true);
    const e = key('keydown', 'KeyM', { ctrlKey: true });
    key('keydown', 'KeyM', { ctrlKey: true, repeat: true });
    expect(onMute).toHaveBeenCalledTimes(1);
    expect(e.defaultPrevented).toBe(true);
    l.detach();
  });
});
