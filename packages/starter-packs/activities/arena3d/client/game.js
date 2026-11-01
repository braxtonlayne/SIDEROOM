// @ts-check
// The running match: one fixed-step loop for the simulation (60 Hz, driven by the page clock; the clock never
// enters sim/), then one render per animation frame. On the host this drives HostSession (the authority); on
// everyone else PredictedClient (prediction + reconciliation). Rendering, the HUD, sound and the spatial voice
// hooks read from whichever it is.

import { World, TICK_RATE } from '../sim/world.js';
import { seedFromString } from '../sim/rng.js';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { mat4, unitsToRad } from '../render/math.js';
import { createScene } from './scene.js';

/** @typedef {import('../sim/world.js').JoltModule} JoltModule */
/** @typedef {import('./scene.js').PlayerView} PlayerView */

const TICK_MS = 1000 / TICK_RATE;
/** Never run more than this many ticks in one frame (a background tab must not spiral when it comes back). */
const MAX_TICKS_PER_FRAME = 8;

export class Game {
  /**
   * @param {{ J: JoltModule, platform: import('../platform/platform.js').Platform, renderer: import('../render/renderer.js').Renderer,
   *   canvas: HTMLCanvasElement, input: import('./input.js').InputState, settings: import('./settings.js').Settings,
   *   hud: import('./hud.js').Hud, sfx: import('./sfx.js').Sfx, voice: import('./spatial-voice.js').SpatialVoice }} opt
   */
  constructor(opt) {
    this.opt = opt;
    const channel = opt.platform.channel;
    if (!channel) throw new Error('no data channel');
    const name = opt.settings.name || opt.platform.identity().name;
    this.world = new World(opt.J, { seed: seedFromString(opt.platform.instanceId()) });
    /** @type {HostSession | null} */
    this.host = channel.isHost ? new HostSession({ channel, world: this.world, name }) : null;
    /** @type {PredictedClient | null} */
    this.client = channel.isHost ? null : new PredictedClient({ channel, world: this.world, name });
    this.scene = createScene(opt.renderer, this.world.level);
    this.acc = 0;
    this.last = -1;
    this.running = false;
    this.frames = 0;
    this.fpsAt = 0;
    this.fps = 0;
    this.lastScore = 0;
    this.lastRoster = 1;
    this.wasGrounded = true;
    /** @type {number} */
    this.raf = 0;
    /** @type {ReturnType<typeof setInterval> | null} the hidden-tab ticker */
    this.timer = null;
  }

  /** The local player's slot (-1 until a peer is welcomed). */
  get slot() {
    return this.host ? this.host.selfSlot : (this.client?.slot ?? -1);
  }

  start() {
    if (this.running) return;
    this.running = true;
    // Face the middle of the arena from the spawn point.
    this.opt.input.look(0);
    const frame = (/** @type {number} */ now) => {
      if (!this.running) return;
      this.raf = requestAnimationFrame(frame);
      this.frame(now);
    };
    this.raf = requestAnimationFrame(frame);
    // requestAnimationFrame stops in a hidden tab (or a display:none iframe), which would freeze the match for
    // everyone when it is the host's. While hidden, tick from a timer instead (browsers throttle background
    // timers, often to 1 Hz, so a hidden host still lags; the frame loop catches up to MAX_TICKS_PER_FRAME).
    this.onVisibility = () => {
      if (document.hidden && !this.timer) this.timer = setInterval(() => this.frame(performance.now(), false), 50);
      else if (!document.hidden && this.timer) {
        clearInterval(this.timer);
        this.timer = null;
      }
    };
    document.addEventListener('visibilitychange', this.onVisibility);
    this.onVisibility();
  }

  /** @param {number} now @param {boolean} [draw] false: simulate only (hidden tab) */
  frame(now, draw = true) {
    if (this.last < 0) this.last = now;
    this.acc += Math.min(250, now - this.last);
    this.last = now;
    let n = 0;
    while (this.acc >= TICK_MS && n < MAX_TICKS_PER_FRAME) {
      this.tick();
      this.acc -= TICK_MS;
      n++;
    }
    if (n === MAX_TICKS_PER_FRAME) this.acc = 0;
    if (!draw) return;
    this.client?.frame();
    this.render(this.acc / TICK_MS);
    this.frames++;
    if (now - this.fpsAt > 1000) {
      this.fps = (this.frames * 1000) / (now - this.fpsAt);
      this.frames = 0;
      this.fpsAt = now;
    }
  }

  tick() {
    const input = this.opt.input.sample();
    if (this.host) this.host.tick(input);
    else this.client?.tick(input);
    this.sounds();
  }

  sounds() {
    const sfx = this.opt.sfx;
    const p = this.world.players[this.slot];
    if (p) {
      const grounded = p.character.GetGroundState() === this.opt.J.EGroundState_OnGround;
      const vy = p.character.GetLinearVelocity().GetY();
      if (this.wasGrounded && !grounded && vy > 3) sfx.jump();
      if (!this.wasGrounded && grounded) sfx.land();
      this.wasGrounded = grounded;
    }
    if (this.world.score > this.lastScore) sfx.score();
    this.lastScore = this.world.score;
    const roster = this.host ? this.host.roster().length : (this.client?.roster.length ?? 0);
    if (roster > this.lastRoster) sfx.join();
    this.lastRoster = roster;
  }

  /** @param {number} alpha fraction of a tick since the last step (for interpolation) */
  render(alpha) {
    const { renderer, canvas, settings, hud, voice } = this.opt;
    const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
    const w = Math.floor(canvas.clientWidth * dpr);
    const h = Math.floor(canvas.clientHeight * dpr);
    if (w > 0 && h > 0 && (canvas.width !== w || canvas.height !== h)) renderer.resize(w, h);

    const me = this.host ? this.world.eye(this.slot) : this.client?.myEye();
    const eye = me ?? { x: 0, y: 6, z: 16, yaw: 0, pitch: -2500, crouched: false };
    // Look where the mouse says right now (smoother than waiting for the next tick).
    const look = this.opt.input.sample();
    const yaw = me ? look.yaw : eye.yaw;
    const pitch = me ? look.pitch : eye.pitch;

    /** @type {PlayerView[]} */
    const others = [];
    for (let slot = 0; slot < this.world.players.length; slot++) {
      if (slot === this.slot) continue;
      if (this.host) {
        const e = this.world.eye(slot);
        if (e) others.push({ slot, x: e.x, y: e.y - (e.crouched ? 0.95 : 1.6), z: e.z, yaw: e.yaw, crouched: e.crouched });
      } else if (this.client) {
        const s = this.client.remote(slot, alpha);
        if (s) others.push({ slot, x: s.p[0], y: s.p[1], z: s.p[2], yaw: s.yaw, crouched: s.crouched });
      }
    }
    const draws = this.scene.draws(this.world.boxTransforms(this.scene.boxScratch), others);
    const yawRad = unitsToRad(yaw);
    const pitchRad = unitsToRad(pitch);
    renderer.frame(
      {
        view: mat4.fpsView([eye.x, eye.y, eye.z], yawRad, pitchRad),
        eye: [eye.x, eye.y, eye.z],
        fovY: (settings.fov * Math.PI) / 180,
        near: 0.05,
        far: 200,
        lightDir: [-0.35, -0.85, -0.4],
        lightColor: [1.0, 0.95, 0.85],
        ambient: [0.32, 0.36, 0.45],
        fogColor: [0.55, 0.68, 0.82],
        fogDensity: 0.012,
      },
      draws,
    );

    // Spatial voice hooks (no-op until `ingame_voice` exists): listener = me, speakers = everyone else.
    const fwd = /** @type {[number, number, number]} */ ([-Math.sin(yawRad) * Math.cos(pitchRad), Math.sin(pitchRad), -Math.cos(yawRad) * Math.cos(pitchRad)]);
    voice.onListenerMove([eye.x, eye.y, eye.z], fwd);
    for (const o of others) voice.onSpeakerMove(`slot:${o.slot}`, [o.x, o.y + 1.6, o.z]);

    hud.update({
      role: this.host ? 'host' : 'peer',
      status: this.host ? 'playing' : (this.client?.status ?? 'joining'),
      score: this.world.score,
      roster: this.host ? this.host.roster() : (this.client?.roster ?? []),
      mySlot: this.slot,
      renderer: renderer.kind,
      fps: this.fps,
      net: this.client
        ? { pending: this.client.pending.length, corrections: this.client.stats.corrections, lastCorrection: this.client.stats.lastCorrection }
        : this.host
          ? { peers: this.host.peers.size, snapshots: this.host.stats.snapshots }
          : null,
      showNet: settings.showNet,
    });
  }

  stop() {
    this.running = false;
    cancelAnimationFrame(this.raf);
    if (this.timer) clearInterval(this.timer);
    if (this.onVisibility) document.removeEventListener('visibilitychange', this.onVisibility);
    this.client?.leave();
    this.host?.close();
  }
}
