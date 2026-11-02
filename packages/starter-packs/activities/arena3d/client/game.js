// @ts-check
// The running match: one fixed-step loop for the simulation (60 Hz, driven by the page clock; the clock never
// enters sim/), then one render per animation frame. On the host this drives HostSession (the authority); on
// everyone else PredictedClient (prediction + reconciliation). Rendering, the HUD, sound and the spatial voice
// hooks read from whichever it is.

import { World } from '../sim/world.js';
import { seedFromString } from '../sim/rng.js';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { mat4, unitsToRad } from '../render/math.js';
import { handoffRole } from '../net/handoff.js';
import { createScene } from './scene.js';
import { TickPacer } from './pacer.js';

/** @typedef {import('../sim/world.js').JoltModule} JoltModule */
/** @typedef {import('./scene.js').PlayerView} PlayerView */

/** More ticks than this in one burst (a hidden host catching up): publish one snapshot at the end, not one per 3 ticks. */
const QUIET_BURST = 4;

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
    this.pacer = new TickPacer();
    /** performance.now() of the last drawn frame (see stalled()). */
    this.lastFrameAt = 0;
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
    /** @type {(() => void)[]} */
    this.offs = [];
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
    // requestAnimationFrame stops in a hidden tab, a display:none iframe or an occluded window, which would freeze
    // the match for everyone when it is the host's. So a watchdog timer ticks whenever no animation frame has run
    // for 100 ms (it does nothing while frames come). Browsers throttle background timers (to 1 Hz, later to once a
    // minute), so a stalled HOST keeps the simulation on time three ways:
    //   1. every call counts the real elapsed time and runs that many fixed ticks in one burst (TickPacer
    //      catch-up, capped at 5 s / 300 ticks), publishing one snapshot at the end of a long burst;
    //   2. every message a peer sends the host also runs the loop (messages are not throttled like timers, and a
    //      match with peers gets about 30 a second from each);
    //   3. when the tab comes back, one more burst runs before the first drawn frame.
    this.timer = setInterval(() => this.stalled() && this.frame(performance.now(), false), 50);
    this.onVisibility = () => {
      if (!document.hidden) this.frame(performance.now(), false);
    };
    document.addEventListener('visibilitychange', this.onVisibility);
    const ch = this.opt.platform.channel;
    if (ch) {
      this.offs.push(ch.onInput(() => this.host && this.stalled() && this.frame(performance.now(), false)));
      const off = ch.onHostChange?.(() => this.handoff());
      if (off) this.offs.push(off);
    }
  }

  /** No animation frame for 100 ms: the tab is hidden or occluded, or the page is stalled. */
  stalled() {
    return performance.now() - this.lastFrameAt > 100;
  }

  /**
   * D-388 (host_handoff): the platform moved the host. A peer that is the new host becomes the host from the last
   * snapshot it heard; a peer that is not says hello to the new one; a host that no longer is the host steps down.
   */
  handoff() {
    const channel = this.opt.platform.channel;
    if (!channel) return;
    const name = this.opt.settings.name || this.opt.platform.identity().name;
    const next = handoffRole({ channel, world: this.world, name, host: this.host, client: this.client });
    this.host = next.host;
    this.client = next.client;
    if (this.host) this.lastRoster = this.host.roster().length;
  }

  /** @param {number} now @param {boolean} [draw] false: simulate only (hidden tab, or catching up) */
  frame(now, draw = true) {
    // Only a host must stay on time; a hidden peer just predicts (and the next snapshot corrects it).
    const n = this.pacer.due(now, !draw && !!this.host);
    for (let i = 0; i < n; i++) this.tick(n <= QUIET_BURST);
    if (n > QUIET_BURST) this.host?.publishSnapshot();
    if (!draw) return;
    this.lastFrameAt = now;
    this.client?.frame();
    this.render(this.pacer.alpha);
    this.frames++;
    if (now - this.fpsAt > 1000) {
      this.fps = (this.frames * 1000) / (now - this.fpsAt);
      this.frames = 0;
      this.fpsAt = now;
    }
  }

  /** @param {boolean} [publish] false: inside a long burst; the caller publishes once at the end */
  tick(publish = true) {
    const input = this.opt.input.sample();
    if (this.host) this.host.tick(input, publish);
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
    const { renderer, settings, hud, voice } = this.opt;
    const canvas = renderer.canvas ?? this.opt.canvas;
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
      hostSlot: this.host ? this.host.selfSlot : (this.client?.hostSlot() ?? 0),
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
    for (const off of this.offs.splice(0)) off();
    if (this.onVisibility) document.removeEventListener('visibilitychange', this.onVisibility);
    this.client?.leave();
    this.host?.close();
  }
}
