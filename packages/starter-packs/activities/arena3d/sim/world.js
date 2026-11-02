// @ts-check
// The deterministic simulation: one Jolt physics world plus the little game state on top of it. No DOM, no
// clock, no Math.random: it runs the same in Node and in every browser, and two Worlds fed the same inputs from
// the same seed stay bit-identical (test/replay.test.js is the proof).
//
// Rules that keep it deterministic (brief §2.4), and where each is enforced:
//   - fixed 60 Hz step with integer tick numbers: `step()` always advances DT and `tick` by one;
//   - inputs are small integers: sim/input.js, and yaw becomes a direction through sim/dmath.js, not Math.sin;
//   - one seeded PRNG (sim/rng.js) whose state is part of the snapshot and the hash;
//   - no wall-clock time anywhere in sim/;
//   - bodies are created, added and stepped in a fixed order (statics, boxes, then players by slot), and the
//     CharacterID counter is reset per world, so IDs do not depend on what ran before in the same module.
//
// The npm Jolt build is not compiled with CROSS_PLATFORM_DETERMINISTIC. The same .wasm runs in every browser,
// so results should still agree; replay/replay.html is how that is checked per browser (see README.md).

import { ARENA } from './level.js';
import { StateHasher } from './hash.js';
import { BTN, idleInput, sanitiseInput } from './input.js';
import { Rng } from './rng.js';
import { sinCos } from './dmath.js';

/** @typedef {ArenaJoltFactory} JoltFactory  (types/jolt.d.ts) */
/** @typedef {ArenaJoltModule} JoltModule */
/** @typedef {import('./input.js').PlayerInput} PlayerInput */
/** @typedef {import('./level.js').Level} Level */
/** @typedef {import('./level.js').Block} Block */

export const TICK_RATE = 60;
export const DT = 1 / TICK_RATE;
export const MAX_PLAYERS = 8;

const LAYER_STATIC = 0;
const LAYER_MOVING = 1;

/** Movement tuning. Change these to change how the game feels. */
export const TUNING = Object.freeze({
  gravity: -20,
  walkSpeed: 6,
  crouchSpeed: 3,
  jumpSpeed: 7.5,
  /** Fraction of the new wish velocity blended in per tick (Jolt's CharacterVirtual sample uses 0.25). */
  accel: 0.25,
  radius: 0.35,
  standHalf: 0.55,
  crouchHalf: 0.2,
  standEye: 1.6,
  crouchEye: 0.95,
  mass: 70,
  /** The most force (N) a player can push with: enough to shove a 20 kg box, not a wall. */
  pushStrength: 400,
  boxDensity: 20,
});

/**
 * One player's state that is not inside Jolt.
 * @typedef {object} Player
 * @property {number} slot
 * @property {any} character      Jolt.CharacterVirtual
 * @property {boolean} crouched
 * @property {number} prevButtons  last tick's buttons (jump fires on the press, not the hold)
 * @property {PlayerInput} input   what this player does on the next step
 * @property {number} yaw
 * @property {number} pitch
 * @property {number} dvx          smoothed wish velocity (float32), x
 * @property {number} dvz          smoothed wish velocity (float32), z
 */

/**
 * @typedef {object} BoxBody
 * @property {any} id                       Jolt.BodyID
 * @property {[number, number, number]} half
 * @property {[number, number, number]} color
 */

/**
 * Everything needed to put a World back into a given tick: what the host sends (net/codec.js) and what the
 * client rewinds to. Floats are float32 values read back from Jolt, so encoding them as float32 is lossless.
 * @typedef {object} PlayerSnap
 * @property {number} slot
 * @property {boolean} crouched
 * @property {number} prevButtons
 * @property {number} yaw
 * @property {number} pitch
 * @property {number} dvx
 * @property {number} dvz
 * @property {[number, number, number]} p
 * @property {[number, number, number]} v
 */
/**
 * @typedef {object} BoxSnap
 * @property {[number, number, number]} p
 * @property {[number, number, number, number]} q
 * @property {[number, number, number]} v
 * @property {[number, number, number]} w
 * @property {boolean} active
 */
/**
 * @typedef {object} Snapshot
 * @property {number} tick
 * @property {number} rng
 * @property {number} score
 * @property {PlayerSnap[]} players   sorted by slot
 * @property {BoxSnap[]} boxes         in creation order
 */

const f32 = Math.fround;

export class World {
  /**
   * @param {JoltModule} J an initialised Jolt module (await of the jolt-physics factory). Give each World its own
   *   module when you need two that cannot share any hidden state (the determinism test does).
   * @param {{ seed: number, level?: Level }} opt
   */
  constructor(J, opt) {
    this.J = J;
    this.level = opt.level ?? ARENA;
    this.seed = opt.seed >>> 0;
    this.rng = new Rng(this.seed);
    this.tick = 0;
    /** Boxes whose centre is on the goal pad, recounted every tick. */
    this.score = 0;
    /** @type {(Player | null)[]} */
    this.players = new Array(MAX_PLAYERS).fill(null);
    /** @type {BoxBody[]} */
    this.boxes = [];

    // Two object layers (static, moving) mapped onto two broad-phase layers. Static never collides with static.
    const pairs = new J.ObjectLayerPairFilterTable(2);
    pairs.EnableCollision(LAYER_STATIC, LAYER_MOVING);
    pairs.EnableCollision(LAYER_MOVING, LAYER_MOVING);
    const bpStatic = new J.BroadPhaseLayer(0);
    const bpMoving = new J.BroadPhaseLayer(1);
    const bpInterface = new J.BroadPhaseLayerInterfaceTable(2, 2);
    bpInterface.MapObjectToBroadPhaseLayer(LAYER_STATIC, bpStatic);
    bpInterface.MapObjectToBroadPhaseLayer(LAYER_MOVING, bpMoving);
    const settings = new J.JoltSettings();
    settings.mObjectLayerPairFilter = pairs;
    settings.mBroadPhaseLayerInterface = bpInterface;
    settings.mObjectVsBroadPhaseLayerFilter = new J.ObjectVsBroadPhaseLayerFilterTable(bpInterface, 2, pairs, 2);
    settings.mMaxBodies = 1024;
    settings.mMaxBodyPairs = 4096;
    settings.mMaxContactConstraints = 2048;
    this.jolt = new J.JoltInterface(settings);
    J.destroy(settings);
    J.destroy(bpStatic);
    J.destroy(bpMoving);
    this.physics = this.jolt.GetPhysicsSystem();
    this.bi = this.physics.GetBodyInterface();
    this.physics.SetGravity(new J.Vec3(0, TUNING.gravity, 0));

    // Scratch objects reused every call, so a step allocates nothing on the WASM heap.
    this.v3 = new J.Vec3(0, 0, 0);
    this.r3 = new J.RVec3(0, 0, 0);
    this.q4 = new J.Quat(0, 0, 0, 1);
    this.v3b = new J.Vec3(0, 0, 0);
    this.gravity = new J.Vec3(0, TUNING.gravity, 0);
    this.updateSettings = new J.ExtendedUpdateSettings();
    this.bpFilter = new J.DefaultBroadPhaseLayerFilter(this.jolt.GetObjectVsBroadPhaseLayerFilter(), LAYER_MOVING);
    this.objFilter = new J.DefaultObjectLayerFilter(this.jolt.GetObjectLayerPairFilter(), LAYER_MOVING);
    this.bodyFilter = new J.BodyFilter();
    this.shapeFilter = new J.ShapeFilter();
    this.tempAlloc = this.jolt.GetTempAllocator();

    // Character shapes, offset so the character's position is at its feet.
    this.standShape = this.capsule(TUNING.standHalf);
    this.crouchShape = this.capsule(TUNING.crouchHalf);

    // CharacterIDs come from a module-wide counter; start it over so a World's IDs never depend on history.
    const cid = new J.CharacterID();
    cid.sSetNextCharacterID(1);
    J.destroy(cid);

    for (const b of this.level.statics) this.addStatic(b);
    for (const b of this.level.boxes) this.addBox(b.pos, b.half, b.color);
    const [ax, az] = this.level.dropArea;
    for (let i = 0; i < this.level.randomBoxes; i++) {
      const pos = /** @type {[number, number, number]} */ ([this.rng.range(-ax, ax), this.rng.range(4, 9), this.rng.range(-az, az)]);
      this.addBox(pos, [0.45, 0.45, 0.45], i % 2 ? [0.85, 0.35, 0.4] : [0.95, 0.8, 0.3]);
    }
    this.physics.OptimizeBroadPhase();
  }

  /** @param {number} half cylinder half height @returns {any} Jolt.Shape */
  capsule(half) {
    const J = this.J;
    const capsule = new J.CapsuleShapeSettings(half, TUNING.radius);
    const offset = new J.Vec3(0, half + TUNING.radius, 0);
    const rot = new J.Quat(0, 0, 0, 1);
    const shapeSettings = new J.RotatedTranslatedShapeSettings(offset, rot, capsule);
    const shape = shapeSettings.Create().Get();
    shape.AddRef();
    J.destroy(offset);
    J.destroy(rot);
    return shape;
  }

  /** A unit quaternion for an integer angle about X. @param {number} units */
  setRotX(units) {
    // Half the angle: units is even in level data, and the half-angle trick keeps it integer either way.
    const [s, c] = sinCos(Math.round(units / 2));
    this.q4.Set(s, 0, 0, c);
    return this.q4;
  }

  /** @param {Block} b */
  addStatic(b) {
    const J = this.J;
    this.v3.Set(b.half[0], b.half[1], b.half[2]);
    const shape = new J.BoxShape(this.v3, 0.05);
    this.r3.Set(b.pos[0], b.pos[1], b.pos[2]);
    const cs = new J.BodyCreationSettings(shape, this.r3, this.setRotX(b.rotX ?? 0), J.EMotionType_Static, LAYER_STATIC);
    cs.mFriction = 0.6;
    const body = this.bi.CreateBody(cs);
    this.bi.AddBody(body.GetID(), J.EActivation_DontActivate);
    J.destroy(cs);
  }

  /**
   * @param {[number, number, number]} pos @param {[number, number, number]} half
   * @param {[number, number, number]} color
   */
  addBox(pos, half, color) {
    const J = this.J;
    this.v3.Set(half[0], half[1], half[2]);
    const shapeSettings = new J.BoxShapeSettings(this.v3, 0.05);
    shapeSettings.mDensity = TUNING.boxDensity;
    const shape = shapeSettings.Create().Get();
    this.r3.Set(pos[0], pos[1], pos[2]);
    const cs = new J.BodyCreationSettings(shape, this.r3, this.setRotX(0), J.EMotionType_Dynamic, LAYER_MOVING);
    cs.mFriction = 0.5;
    cs.mRestitution = 0.1;
    const body = this.bi.CreateBody(cs);
    const id = body.GetID();
    this.bi.AddBody(id, J.EActivation_Activate);
    J.destroy(cs);
    this.boxes.push({ id, half: [half[0], half[1], half[2]], color });
  }

  /**
   * Put a player into a slot (at its spawn point). Call between steps; adding players in slot order at the same
   * tick on every client keeps the worlds identical.
   * @param {number} slot 0..MAX_PLAYERS-1
   */
  addPlayer(slot) {
    if (this.players[slot]) return this.players[slot];
    const J = this.J;
    const s = new J.CharacterVirtualSettings();
    s.mMass = TUNING.mass;
    s.mMaxStrength = TUNING.pushStrength;
    s.mShape = this.standShape;
    s.mInnerBodyShape = this.standShape;
    s.mInnerBodyLayer = LAYER_MOVING;
    s.mMaxSlopeAngle = 0.7853981633974483; // 45 degrees, as a literal
    s.mCharacterPadding = 0.02;
    s.mPenetrationRecoverySpeed = 1;
    s.mPredictiveContactDistance = 0.1;
    this.v3.Set(0, 1, 0);
    const plane = new J.Plane(this.v3, -TUNING.radius);
    s.mSupportingVolume = plane;
    const [x, y, z] = this.level.spawns[slot % this.level.spawns.length];
    this.r3.Set(x, y, z);
    const character = new J.CharacterVirtual(s, this.r3, this.setRotX(0), this.physics);
    J.destroy(plane);
    /** @type {Player} */
    const p = { slot, character, crouched: false, prevButtons: 0, input: idleInput(), yaw: 0, pitch: 0, dvx: 0, dvz: 0 };
    this.players[slot] = p;
    return p;
  }

  /** @param {number} slot */
  removePlayer(slot) {
    const p = this.players[slot];
    if (!p) return;
    this.J.destroy(p.character);
    this.players[slot] = null;
  }

  /** The input a player uses on the next step (sticky until changed). @param {number} slot @param {PlayerInput} input */
  setInput(slot, input) {
    const p = this.players[slot];
    if (p) p.input = sanitiseInput(input);
  }

  /** Advance one fixed tick. */
  step() {
    for (const p of this.players) if (p) this.movePlayer(p);
    this.jolt.Step(DT, 1);
    this.afterStep();
    this.tick++;
  }

  /** @param {Player} p */
  movePlayer(p) {
    const J = this.J;
    const ch = p.character;
    const b = p.input.buttons;
    p.yaw = p.input.yaw;
    p.pitch = p.input.pitch;

    // Crouch: swap the capsule. Standing up fails (and the player stays crouched) when something is overhead.
    const wantCrouch = (b & BTN.CROUCH) !== 0;
    if (wantCrouch !== p.crouched) {
      const shape = wantCrouch ? this.crouchShape : this.standShape;
      if (ch.SetShape(shape, 0.03, this.bpFilter, this.objFilter, this.bodyFilter, this.shapeFilter, this.tempAlloc)) {
        ch.SetInnerBodyShape(shape);
        p.crouched = wantCrouch;
      }
    }

    // Wish direction from the integer yaw. Yaw 0 looks down -Z; right is +X.
    const [s, c] = sinCos(p.yaw);
    const fwd = ((b & BTN.FORWARD) ? 1 : 0) - ((b & BTN.BACK) ? 1 : 0);
    const side = ((b & BTN.RIGHT) ? 1 : 0) - ((b & BTN.LEFT) ? 1 : 0);
    let wx = -s * fwd + c * side;
    let wz = -c * fwd - s * side;
    const len2 = wx * wx + wz * wz;
    if (len2 > 1) {
      const inv = 1 / Math.sqrt(len2); // sqrt is correctly rounded in IEEE-754, so it is deterministic
      wx *= inv;
      wz *= inv;
    }
    const speed = p.crouched ? TUNING.crouchSpeed : TUNING.walkSpeed;
    p.dvx = f32(TUNING.accel * wx * speed + (1 - TUNING.accel) * p.dvx);
    p.dvz = f32(TUNING.accel * wz * speed + (1 - TUNING.accel) * p.dvz);

    // Vertical: Jolt's CharacterVirtual recipe. On the ground, take the ground's velocity (and jump from it);
    // in the air, keep falling.
    ch.UpdateGroundVelocity();
    const gv = ch.GetGroundVelocity();
    const gx = gv.GetX();
    const gy = gv.GetY();
    const gz = gv.GetZ();
    const cy = ch.GetLinearVelocity().GetY();
    const onGround = ch.GetGroundState() === J.EGroundState_OnGround;
    let nx = 0;
    let ny = cy;
    let nz = 0;
    if (onGround && cy - gy < 0.1) {
      nx = gx;
      ny = gy;
      nz = gz;
      const jumpPressed = (b & BTN.JUMP) !== 0 && (p.prevButtons & BTN.JUMP) === 0;
      if (jumpPressed && !p.crouched) ny += TUNING.jumpSpeed;
    }
    ny += TUNING.gravity * DT;
    nx += p.dvx;
    nz += p.dvz;
    this.v3.Set(nx, ny, nz);
    ch.SetLinearVelocity(this.v3);
    ch.ExtendedUpdate(DT, this.gravity, this.updateSettings, this.bpFilter, this.objFilter, this.bodyFilter, this.shapeFilter, this.tempAlloc);
    p.prevButtons = b;
  }

  /** Game rules after physics: respawn what fell out, count boxes on the pad. */
  afterStep() {
    const J = this.J;
    const lv = this.level;
    let score = 0;
    for (const box of this.boxes) {
      const pos = this.bi.GetPosition(box.id);
      const x = pos.GetX();
      const y = pos.GetY();
      const z = pos.GetZ();
      if (y < lv.killY) {
        // Drop it back in at a PRNG-chosen spot (seeded, so every client picks the same one).
        const [ax, az] = lv.dropArea;
        this.r3.Set(this.rng.range(-ax, ax), 8, this.rng.range(-az, az));
        this.v3.Set(0, 0, 0);
        this.bi.SetPositionRotationAndVelocity(box.id, this.r3, this.setRotX(0), this.v3, this.v3);
        this.bi.ActivateBody(box.id);
        continue;
      }
      if (x >= lv.pad.min[0] && x <= lv.pad.max[0] && z >= lv.pad.min[1] && z <= lv.pad.max[1] && y < 2) score++;
    }
    this.score = score;
    for (const p of this.players) {
      if (!p || p.character.GetPosition().GetY() >= lv.killY) continue;
      const [x, y, z] = lv.spawns[p.slot % lv.spawns.length];
      this.r3.Set(x, y, z);
      p.character.SetPosition(this.r3);
      this.v3.Set(0, 0, 0);
      p.character.SetLinearVelocity(this.v3);
    }
    void J;
  }

  /**
   * A stable hash of the whole simulation state: tick, PRNG, score, then every player (by slot) and every box
   * (in creation order), read back from Jolt. `exact: false` (the default, the gate's hash) quantises
   * positions/velocities to 0.1 mm(/s) and rotations to 1e-6; `exact: true` hashes the float32 bit patterns, the
   * strictest comparison.
   * @param {{ exact?: boolean }} [opt]
   * @returns {string} 16 hex digits
   */
  stateHash(opt = {}) {
    const h = new StateHasher();
    const exact = !!opt.exact;
    const P = 1e4;
    const R = 1e6;
    /** @param {number} v @param {number} scale */
    const num = (v, scale) => (exact ? h.f32(v) : h.q(v, scale));
    h.u32(this.tick).u32(this.rng.state).u32(this.score);
    for (const p of this.players) {
      if (!p) continue;
      const ch = p.character;
      const pos = ch.GetPosition();
      h.u32(p.slot).u32(p.crouched ? 1 : 0).u32(p.prevButtons).u32(p.yaw).u32(p.pitch);
      num(pos.GetX(), P), num(pos.GetY(), P), num(pos.GetZ(), P);
      const vel = ch.GetLinearVelocity();
      num(vel.GetX(), P), num(vel.GetY(), P), num(vel.GetZ(), P);
      num(p.dvx, P), num(p.dvz, P);
      h.u32(ch.GetGroundState());
    }
    for (const box of this.boxes) {
      const pos = this.bi.GetPosition(box.id);
      num(pos.GetX(), P), num(pos.GetY(), P), num(pos.GetZ(), P);
      const q = this.bi.GetRotation(box.id);
      num(q.GetX(), R), num(q.GetY(), R), num(q.GetZ(), R), num(q.GetW(), R);
      const v = this.bi.GetLinearVelocity(box.id);
      num(v.GetX(), P), num(v.GetY(), P), num(v.GetZ(), P);
      const w = this.bi.GetAngularVelocity(box.id);
      num(w.GetX(), P), num(w.GetY(), P), num(w.GetZ(), P);
      h.u32(this.bi.IsActive(box.id) ? 1 : 0);
    }
    return h.hex();
  }

  /** The state as plain data (see Snapshot). @returns {Snapshot} */
  snapshot() {
    /** @type {PlayerSnap[]} */
    const players = [];
    for (const p of this.players) {
      if (!p) continue;
      const pos = p.character.GetPosition();
      const px = pos.GetX(), py = pos.GetY(), pz = pos.GetZ();
      const vel = p.character.GetLinearVelocity();
      players.push({
        slot: p.slot, crouched: p.crouched, prevButtons: p.prevButtons, yaw: p.yaw, pitch: p.pitch,
        dvx: p.dvx, dvz: p.dvz, p: [px, py, pz], v: [vel.GetX(), vel.GetY(), vel.GetZ()],
      });
    }
    /** @type {BoxSnap[]} */
    const boxes = this.boxes.map((box) => {
      const pos = this.bi.GetPosition(box.id);
      const p = /** @type {[number, number, number]} */ ([pos.GetX(), pos.GetY(), pos.GetZ()]);
      const q = this.bi.GetRotation(box.id);
      const qq = /** @type {[number, number, number, number]} */ ([q.GetX(), q.GetY(), q.GetZ(), q.GetW()]);
      const v = this.bi.GetLinearVelocity(box.id);
      const vv = /** @type {[number, number, number]} */ ([v.GetX(), v.GetY(), v.GetZ()]);
      const w = this.bi.GetAngularVelocity(box.id);
      const ww = /** @type {[number, number, number]} */ ([w.GetX(), w.GetY(), w.GetZ()]);
      return { p, q: qq, v: vv, w: ww, active: this.bi.IsActive(box.id) };
    });
    return { tick: this.tick, rng: this.rng.state, score: this.score, players, boxes };
  }

  /**
   * Rewind (or fast-forward) to a snapshot: players are added or removed to match it, then every body and
   * player takes the snapshot's values. Jolt's internal caches (contact warm-starting, sleep timers) are not
   * part of a snapshot, so re-simulating from here is very close to, not bit-identical with, the host; the host
   * stays the authority and the next snapshot corrects any drift.
   * @param {Snapshot} snap
   */
  applySnapshot(snap) {
    const J = this.J;
    this.tick = snap.tick;
    this.rng.state = snap.rng >>> 0;
    this.score = snap.score;
    const want = new Set(snap.players.map((p) => p.slot));
    for (let slot = 0; slot < MAX_PLAYERS; slot++) if (this.players[slot] && !want.has(slot)) this.removePlayer(slot);
    for (const ps of snap.players) {
      const p = this.addPlayer(ps.slot);
      if (!p) continue;
      const ch = p.character;
      if (ps.crouched !== p.crouched) {
        const shape = ps.crouched ? this.crouchShape : this.standShape;
        ch.SetShape(shape, 1e9, this.bpFilter, this.objFilter, this.bodyFilter, this.shapeFilter, this.tempAlloc);
        ch.SetInnerBodyShape(shape);
        p.crouched = ps.crouched;
      }
      p.prevButtons = ps.prevButtons;
      p.yaw = ps.yaw;
      p.pitch = ps.pitch;
      p.dvx = ps.dvx;
      p.dvz = ps.dvz;
      p.input = { buttons: ps.prevButtons, yaw: ps.yaw, pitch: ps.pitch };
      this.r3.Set(ps.p[0], ps.p[1], ps.p[2]);
      ch.SetPosition(this.r3);
      this.v3.Set(ps.v[0], ps.v[1], ps.v[2]);
      ch.SetLinearVelocity(this.v3);
      ch.RefreshContacts(this.bpFilter, this.objFilter, this.bodyFilter, this.shapeFilter, this.tempAlloc);
    }
    snap.boxes.forEach((bs, i) => {
      const box = this.boxes[i];
      if (!box) return;
      this.r3.Set(bs.p[0], bs.p[1], bs.p[2]);
      this.q4.Set(bs.q[0], bs.q[1], bs.q[2], bs.q[3]);
      this.v3.Set(bs.v[0], bs.v[1], bs.v[2]);
      this.v3b.Set(bs.w[0], bs.w[1], bs.w[2]);
      this.bi.SetPositionRotationAndVelocity(box.id, this.r3, this.q4, this.v3, this.v3b);
      if (bs.active) this.bi.ActivateBody(box.id);
      else this.bi.DeactivateBody(box.id);
    });
    void J;
  }

  /**
   * Where a player's eye is (for the camera and the voice listener). @param {number} slot
   * @returns {{ x: number, y: number, z: number, yaw: number, pitch: number, crouched: boolean } | null}
   */
  eye(slot) {
    const p = this.players[slot];
    if (!p) return null;
    const pos = p.character.GetPosition();
    return {
      x: pos.GetX(),
      y: pos.GetY() + (p.crouched ? TUNING.crouchEye : TUNING.standEye),
      z: pos.GetZ(),
      yaw: p.yaw,
      pitch: p.pitch,
      crouched: p.crouched,
    };
  }

  /**
   * Every box's transform, for the renderer. Writes into `out` (reused between frames).
   * @param {{ p: number[], q: number[], half: number[], color: number[] }[]} [out]
   */
  boxTransforms(out = []) {
    this.boxes.forEach((box, i) => {
      const pos = this.bi.GetPosition(box.id);
      const q = this.bi.GetRotation(box.id);
      const o = out[i] ?? (out[i] = { p: [0, 0, 0], q: [0, 0, 0, 1], half: box.half, color: box.color });
      o.p[0] = pos.GetX(); o.p[1] = pos.GetY(); o.p[2] = pos.GetZ();
      o.q[0] = q.GetX(); o.q[1] = q.GetY(); o.q[2] = q.GetZ(); o.q[3] = q.GetW();
    });
    out.length = this.boxes.length;
    return out;
  }

  /** Free the WASM-side objects. The World is unusable afterwards. */
  destroy() {
    const J = this.J;
    for (let slot = 0; slot < MAX_PLAYERS; slot++) this.removePlayer(slot);
    for (const o of [this.v3, this.v3b, this.r3, this.q4, this.gravity, this.updateSettings, this.bpFilter, this.objFilter, this.bodyFilter, this.shapeFilter]) J.destroy(o);
    this.standShape.Release();
    this.crouchShape.Release();
    J.destroy(this.jolt);
  }
}
