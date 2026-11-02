// @ts-check
// Runs a recorded input script through a fresh World and returns the state hashes: one every `every` ticks and
// one at the end. The same function runs in Node (test/replay.test.js, replay/run-node.mjs) and in the browser
// (replay/replay.html), so a hash that differs between them points at the engine, not the harness.

import { World } from '../sim/world.js';

/**
 * A recorded input script. `events` are input changes: from `tick` on, `slot` holds that input until its next
 * event (inputs are sticky, like held keys).
 * @typedef {object} ReplayScript
 * @property {number} version
 * @property {number} seed
 * @property {number} ticks            how many ticks to run
 * @property {number} players          slots 0..players-1 join at tick 0
 * @property {[number, number, number, number, number][]} events  [tick, slot, buttons, yaw, pitch], sorted by tick then slot
 */

/**
 * @typedef {object} ReplayResult
 * @property {{ tick: number, hash: string, exact: string }[]} checkpoints
 * @property {string} final          the quantised state hash after the last tick
 * @property {string} finalExact     the float32-exact hash after the last tick
 * @property {number} ticks
 * @property {number} score
 * @property {number[]} boxMoved     how far each box ended up from where it started (m)
 */

/**
 * @param {import('../sim/world.js').JoltModule} J an initialised Jolt module
 * @param {ReplayScript} script
 * @param {{ every?: number, onTick?: (world: World) => void }} [opt]
 * @returns {ReplayResult}
 */
export function runReplay(J, script, opt = {}) {
  const every = opt.every ?? 60;
  const world = new World(J, { seed: script.seed });
  try {
    for (let slot = 0; slot < script.players; slot++) world.addPlayer(slot);
    const start = world.boxTransforms().map((b) => [...b.p]);
    /** @type {ReplayResult['checkpoints']} */
    const checkpoints = [];
    let e = 0;
    for (let t = 0; t < script.ticks; t++) {
      while (e < script.events.length && script.events[e][0] <= t) {
        const [, slot, buttons, yaw, pitch] = script.events[e++];
        world.setInput(slot, { buttons, yaw, pitch });
      }
      world.step();
      opt.onTick?.(world);
      if (world.tick % every === 0) checkpoints.push({ tick: world.tick, hash: world.stateHash(), exact: world.stateHash({ exact: true }) });
    }
    const end = world.boxTransforms();
    const boxMoved = end.map((b, i) => Math.hypot(b.p[0] - start[i][0], b.p[1] - start[i][1], b.p[2] - start[i][2]));
    return {
      checkpoints,
      final: world.stateHash(),
      finalExact: world.stateHash({ exact: true }),
      ticks: world.tick,
      score: world.score,
      boxMoved,
    };
  } finally {
    world.destroy();
  }
}
