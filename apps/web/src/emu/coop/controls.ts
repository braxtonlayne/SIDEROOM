import type { CoopGuest } from './guest';
import { KeyboardPad, maskFromGamepad, type GamepadLike } from './input';

/** True for fields where typing must stay typing. */
export function isEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.tagName !== 'string') return false;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable === true;
}

export interface ControlsEnv {
  addEventListener: Window['addEventListener'];
  removeEventListener: Window['removeEventListener'];
  getGamepads?: () => ArrayLike<GamepadLike | null>;
}

/**
 * Turns the guest's keyboard and gamepads into the pad mask while they watch the host's shared screen.
 * Keys that belong to the pad are swallowed (so arrows do not scroll the page) unless the focus is in a text
 * field, and a window blur releases everything so no button stays held.
 */
export class GuestControls {
  private readonly keys = new KeyboardPad();
  private poll: ReturnType<typeof setInterval> | null = null;
  private attached = false;
  private padMask = 0;

  constructor(
    private readonly guest: Pick<CoopGuest, 'setButtons'>,
    private readonly env: ControlsEnv = window,
  ) {}

  private readonly onDown = (e: Event) => this.key(e as KeyboardEvent, true);
  private readonly onUp = (e: Event) => this.key(e as KeyboardEvent, false);
  private readonly onBlur = () => {
    this.keys.clear();
    this.padMask = 0;
    this.push();
  };

  attach(): void {
    if (this.attached) return;
    this.attached = true;
    this.env.addEventListener('keydown', this.onDown, true);
    this.env.addEventListener('keyup', this.onUp, true);
    this.env.addEventListener('blur', this.onBlur);
    if (this.env.getGamepads) this.poll = setInterval(() => this.readPads(), 16);
  }

  detach(): void {
    if (!this.attached) return;
    this.attached = false;
    this.env.removeEventListener('keydown', this.onDown, true);
    this.env.removeEventListener('keyup', this.onUp, true);
    this.env.removeEventListener('blur', this.onBlur);
    if (this.poll) clearInterval(this.poll);
    this.poll = null;
    this.keys.clear();
    this.padMask = 0;
    this.guest.setButtons(0);
  }

  private key(e: KeyboardEvent, down: boolean): void {
    // A keyup always releases, whatever has focus now (never leave a key held); a keydown with a shortcut
    // modifier or in a text field is not the game's.
    if (down && (e.ctrlKey || e.metaKey || e.altKey || isEditable(e.target))) return;
    const mine = down ? this.keys.press(e.code) : this.keys.release(e.code);
    if (!mine) return;
    e.preventDefault();
    e.stopPropagation();
    this.push();
  }

  private readPads(): void {
    let mask = 0;
    for (const p of Array.from(this.env.getGamepads?.() ?? [])) {
      if (p) mask |= maskFromGamepad(p);
    }
    if (mask !== this.padMask) {
      this.padMask = mask;
      this.push();
    }
  }

  private push(): void {
    this.guest.setButtons(this.keys.mask | this.padMask);
  }
}
