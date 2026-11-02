// @ts-check
// How many fixed simulation ticks are due, given the page clock. The clock decides *when* to step, never *how
// much* (sim/ always advances 1/60 s); this is the one place that decision lives, so it is testable without a
// browser.
//
// Visible tab: rAF calls every frame; at most `maxTicks` (8) ticks per call and at most `maxFrameMs` (250) of
// elapsed time counted, so a stall (the debugger, a slow frame) never makes the page spiral.
//
// Hidden tab: browsers stop rAF and throttle timers (to 1 Hz, and to once a minute after about five minutes). A
// hidden *host* would then run its authoritative simulation slower than real time and every peer would lag with
// it. So a catch-up call (`catchUp: true`) counts up to `catchUpMs` (5 s) of elapsed time and runs up to
// `catchUpTicks` (300) fixed ticks in one go. A longer gap is cut at the cap: the match then runs behind by the
// excess, which is the documented limit (README, "Hidden tabs").

export const TICK_RATE = 60;

export class TickPacer {
  /** @param {{ tickMs?: number, maxFrameMs?: number, maxTicks?: number, catchUpMs?: number, catchUpTicks?: number }} [opt] */
  constructor(opt = {}) {
    this.tickMs = opt.tickMs ?? 1000 / TICK_RATE;
    this.maxFrameMs = opt.maxFrameMs ?? 250;
    this.maxTicks = opt.maxTicks ?? 8;
    this.catchUpMs = opt.catchUpMs ?? 5000;
    this.catchUpTicks = opt.catchUpTicks ?? 300;
    this.last = -1;
    this.acc = 0;
    /** Ticks given up because a gap exceeded a cap: how far behind real time a hidden host fell. */
    this.dropped = 0;
  }

  /**
   * @param {number} now   ms on any monotonic clock
   * @param {boolean} [catchUp] this call may run a long burst (hidden tab, or just back from one)
   * @returns {number} ticks to run now
   */
  due(now, catchUp = false) {
    if (this.last < 0) this.last = now;
    const gap = Math.max(0, now - this.last);
    this.last = now;
    const capMs = catchUp ? this.catchUpMs : this.maxFrameMs;
    this.acc += Math.min(capMs, gap);
    this.dropped += Math.floor(Math.max(0, gap - capMs) / this.tickMs);
    let n = Math.floor(this.acc / this.tickMs);
    const cap = catchUp ? this.catchUpTicks : this.maxTicks;
    if (n >= cap) {
      this.dropped += n - cap;
      n = cap;
      this.acc = 0;
    } else this.acc -= n * this.tickMs;
    return n;
  }

  /** Fraction of a tick left over, for render interpolation. */
  get alpha() {
    return this.acc / this.tickMs;
  }
}
