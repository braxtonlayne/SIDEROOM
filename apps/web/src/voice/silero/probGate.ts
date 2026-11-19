/**
 * D-1050: the probability gate. Turns the detector's speech probability into "open" or "closed", pure and
 * clock-injected like `VadGate`. Opens at `OPEN_PROB` or more; closes only after the probability has stayed under
 * `CLOSE_PROB` for `HANG_MS` (between the two thresholds the state is kept, which is the hysteresis).
 */
import { HANG_MS } from '../vad';

export const OPEN_PROB = 0.5;
export const CLOSE_PROB = 0.35;

export class ProbGate {
  private open_ = false;
  /** When the probability last stayed at or above `CLOSE_PROB` (while open). */
  private lastActiveAt = -Infinity;

  get isOpen(): boolean {
    return this.open_;
  }

  /** Feed one probability reading; returns whether the gate is open afterwards. */
  process(prob: number, nowMs: number): boolean {
    if (prob >= OPEN_PROB) {
      this.open_ = true;
      this.lastActiveAt = nowMs;
    } else if (prob >= CLOSE_PROB) {
      // Between the thresholds: not speech enough to open, enough to keep an open gate from closing.
      if (this.open_) this.lastActiveAt = nowMs;
    } else if (this.open_ && nowMs - this.lastActiveAt > HANG_MS) {
      this.open_ = false;
    }
    return this.open_;
  }

  reset(): void {
    this.open_ = false;
    this.lastActiveAt = -Infinity;
  }
}
