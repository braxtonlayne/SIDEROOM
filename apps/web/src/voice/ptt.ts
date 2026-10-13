/**
 * Push-to-talk and press-to-act key bindings (VOX-CLI-003).
 *
 * Browsers only deliver keys while the tab is focused, so these are tab-focused binds (a global PTT needs a
 * native client, documented in the spec). The listeners run in the capture phase and swallow a bound key, so
 * the PTT key never reaches the composer or any other shortcut handler.
 */
import { endsOnKeyup, matchesKeydown, type Keybind } from './keybind';

type KeyHandlerTarget = Pick<Window, 'addEventListener' | 'removeEventListener'>;

/** While the keybind recorder is open every bind is suspended so recording a key never triggers it. */
let recordingDepth = 0;
export function suspendKeybinds(): () => void {
  recordingDepth++;
  let done = false;
  return () => {
    if (!done) {
      done = true;
      recordingDepth--;
    }
  };
}
const suspended = () => recordingDepth > 0;

function swallow(e: KeyboardEvent): void {
  e.preventDefault();
  e.stopImmediatePropagation();
}

export interface PttOptions {
  bind: Keybind | null;
  /** Milliseconds the transmit state lingers after the key is released. */
  releaseMs: number;
  /** False when not in a voice channel, or the user is on Voice Activity: the key then types normally. */
  enabled: boolean;
}

export class PttController {
  private opts: PttOptions = { bind: null, releaseMs: 0, enabled: false };
  private active = false;
  private down = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private attached = false;

  constructor(
    private readonly onChange: (active: boolean) => void,
    private readonly target: KeyHandlerTarget = window,
  ) {}

  get isActive(): boolean {
    return this.active;
  }

  attach(): void {
    if (this.attached) return;
    this.attached = true;
    this.target.addEventListener('keydown', this.onKeyDown, true);
    this.target.addEventListener('keyup', this.onKeyUp, true);
    this.target.addEventListener('blur', this.onBlur);
  }

  detach(): void {
    if (!this.attached) return;
    this.attached = false;
    this.target.removeEventListener('keydown', this.onKeyDown, true);
    this.target.removeEventListener('keyup', this.onKeyUp, true);
    this.target.removeEventListener('blur', this.onBlur);
    this.release(true);
  }

  configure(opts: PttOptions): void {
    this.opts = opts;
    if (!opts.enabled || !opts.bind) this.release(true);
  }

  private set(active: boolean): void {
    if (this.active === active) return;
    this.active = active;
    this.onChange(active);
  }

  private release(now: boolean): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.down = false;
    if (!this.active) return;
    if (now || this.opts.releaseMs <= 0) {
      this.set(false);
      return;
    }
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.set(false);
    }, this.opts.releaseMs);
  }

  private readonly onKeyDown = (e: KeyboardEvent): void => {
    const { bind, enabled } = this.opts;
    if (!enabled || !bind || suspended() || !matchesKeydown(bind, e)) return;
    swallow(e);
    this.down = true;
    clearTimeout(this.timer);
    this.timer = undefined;
    this.set(true);
  };

  private readonly onKeyUp = (e: KeyboardEvent): void => {
    const { bind, enabled } = this.opts;
    if (!enabled || !bind || !this.down || !endsOnKeyup(bind, e)) return;
    if (e.code === bind.code) swallow(e);
    this.release(false);
  };

  /** Losing focus means key-up events will never arrive: stop transmitting at once. */
  private readonly onBlur = (): void => this.release(true);
}

export type ShortcutHandlers = Partial<Record<string, () => void>>;

/**
 * Fires an action once per key press (mute, deafen, disconnect). Bound keys are swallowed only while
 * `enabled`, so outside a call they behave normally.
 */
export class ShortcutListener {
  private binds: Record<string, Keybind | null> = {};
  private handlers: ShortcutHandlers = {};
  private enabled = false;
  private attached = false;

  constructor(private readonly target: KeyHandlerTarget = window) {}

  configure(binds: Record<string, Keybind | null>, handlers: ShortcutHandlers, enabled: boolean) {
    this.binds = binds;
    this.handlers = handlers;
    this.enabled = enabled;
  }

  attach(): void {
    if (this.attached) return;
    this.attached = true;
    this.target.addEventListener('keydown', this.onKeyDown, true);
  }

  detach(): void {
    if (!this.attached) return;
    this.attached = false;
    this.target.removeEventListener('keydown', this.onKeyDown, true);
  }

  private readonly onKeyDown = (e: KeyboardEvent): void => {
    if (!this.enabled || suspended()) return;
    for (const [action, bind] of Object.entries(this.binds)) {
      if (!bind || !matchesKeydown(bind, e)) continue;
      const run = this.handlers[action];
      if (!run) continue;
      swallow(e);
      if (!e.repeat) run();
      return;
    }
  };
}
