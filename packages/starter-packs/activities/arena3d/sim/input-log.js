// @ts-check
// A ring buffer of input changes in the replay script's event format, [tick, slot, buttons, yaw, pitch] (see
// replay/run.js). The host records every input it applies that differs from the slot's previous one, so the
// last few thousand events plus the state hash are what a crash report or a bug replay needs (the app's crash
// reporter, wave 45, already records the bridge commands; this is the game's own half).

export class InputLog {
  /** @param {number} [capacity] */
  constructor(capacity = 4096) {
    this.capacity = capacity;
    /** @type {[number, number, number, number, number][]} */
    this.events = [];
    /** @type {Map<number, string>} last input per slot, to log changes only */
    this.last = new Map();
  }

  /**
   * @param {number} tick the tick the input applies to @param {number} slot
   * @param {import('./input.js').PlayerInput} input
   */
  record(tick, slot, input) {
    const key = `${input.buttons},${input.yaw},${input.pitch}`;
    if (this.last.get(slot) === key) return;
    this.last.set(slot, key);
    this.events.push([tick, slot, input.buttons, input.yaw, input.pitch]);
    if (this.events.length > this.capacity) this.events.splice(0, this.events.length - this.capacity);
  }

  /** A slot left: its next input is logged even if it equals the last one. @param {number} slot */
  forget(slot) {
    this.last.delete(slot);
  }

  /** A copy of the events, oldest first. */
  snapshot() {
    return this.events.map((e) => /** @type {[number, number, number, number, number]} */ ([...e]));
  }
}
