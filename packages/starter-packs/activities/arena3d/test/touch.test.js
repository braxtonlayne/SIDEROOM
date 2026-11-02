// @ts-check
// client/touch.js: the stick, the look drag and the buttons become the same integer input the keyboard and mouse
// make, so the simulation, the wire format and replay logs cannot tell a phone from a keyboard.
import assert from 'node:assert/strict';
import { InputState } from '../client/input.js';
import { STICK_RADIUS, TOUCH_LOOK_SCALE, TouchController, stickButtons } from '../client/touch.js';
import { BTN, PITCH_LIMIT, sanitiseInput } from '../sim/input.js';
import { decode, encodeInput } from '../net/codec.js';

/** An InputState with no browser around it. */
function makeInput() {
  const doc = { addEventListener() {}, removeEventListener() {}, pointerLockElement: null };
  const win = { document: doc, addEventListener() {}, removeEventListener() {} };
  return new InputState(/** @type {any} */ (win), /** @type {any} */ ({}), { sensitivity: 5, invertY: false });
}

const R = STICK_RADIUS;

test('the stick reads as 8 directions like WASD, with a dead zone', () => {
  assert.equal(stickButtons(0, 0), 0);
  assert.equal(stickButtons(R * 0.2, 0), 0, 'inside the dead zone');
  assert.equal(stickButtons(0, -R), BTN.FORWARD);
  assert.equal(stickButtons(0, R), BTN.BACK);
  assert.equal(stickButtons(-R, 0), BTN.LEFT);
  assert.equal(stickButtons(R, 0), BTN.RIGHT);
  assert.equal(stickButtons(R, -R), BTN.FORWARD | BTN.RIGHT);
  assert.equal(stickButtons(-R, -R), BTN.FORWARD | BTN.LEFT);
  assert.equal(stickButtons(R, R), BTN.BACK | BTN.RIGHT);
  assert.equal(stickButtons(-R, R), BTN.BACK | BTN.LEFT);
  // Slightly off an axis is still that axis (22.5 degrees either side).
  assert.equal(stickButtons(R * 0.2, -R), BTN.FORWARD);
  assert.equal(stickButtons(R * 0.5, -R), BTN.FORWARD | BTN.RIGHT);
});

test('thumbs drive the input: stick to bits, drag to yaw/pitch, buttons to jump and crouch', () => {
  const input = makeInput();
  input.setTouch(true);
  const t = new TouchController(input);
  assert.equal(input.sample().buttons, 0);
  // Left thumb down at (100, 600), pushed up-right.
  t.stickDown(1, 100, 600);
  t.stickMove(1, 100 + R, 600 - R);
  assert.equal(input.sample().buttons, BTN.FORWARD | BTN.RIGHT);
  // Right thumb drags 40 px right and 10 px up: turn right (yaw falls like a mouse moving right), look up.
  t.lookDown(2, 300, 400);
  t.lookMove(2, 340, 390);
  const s = input.sample();
  assert.ok(s.yaw < 0 && s.pitch > 0, `yaw ${s.yaw}, pitch ${s.pitch}`);
  assert.equal(s.yaw, Math.round(-40 * 5.5 * 5 * TOUCH_LOOK_SCALE));
  assert.ok(Number.isInteger(s.yaw) && Number.isInteger(s.pitch));
  // A third finger on Jump, then Crouch toggles on and stays on.
  t.buttonDown(3, BTN.JUMP);
  assert.equal(input.sample().buttons, BTN.FORWARD | BTN.RIGHT | BTN.JUMP);
  t.up(3);
  t.toggleCrouch();
  assert.equal(input.sample().buttons, BTN.FORWARD | BTN.RIGHT | BTN.CROUCH);
  t.up(1);
  assert.equal(input.sample().buttons, BTN.CROUCH, 'the stick let go; crouch stays until tapped again');
  t.toggleCrouch();
  assert.equal(input.sample().buttons, 0);
});

test('the stick knob stops at full deflection and the thumb can slide on', () => {
  const input = makeInput();
  input.setTouch(true);
  const t = new TouchController(input);
  t.stickDown(1, 100, 100);
  t.stickMove(1, 100 + 500, 100);
  assert.ok(t.stick && Math.hypot(t.stick.x - 100, t.stick.y - 100) <= R + 1e-9);
  assert.equal(input.sample().buttons, BTN.RIGHT);
  t.stickMove(1, 100, 100 - 500);
  assert.equal(input.sample().buttons, BTN.FORWARD, 'swinging the thumb round changes direction at once');
});

test('only one finger holds the stick and one looks; another finger cannot steal them', () => {
  const input = makeInput();
  input.setTouch(true);
  const t = new TouchController(input);
  assert.equal(t.stickDown(1, 50, 500), true);
  assert.equal(t.stickDown(2, 60, 510), false);
  t.stickMove(2, 400, 500); // the second finger's moves are ignored
  assert.equal(input.sample().buttons, 0);
  assert.equal(t.lookDown(3, 300, 300), true);
  assert.equal(t.lookDown(4, 310, 310), false);
});

test('touch input only counts while the touch controls are on, and pitch stays inside its limits', () => {
  const input = makeInput();
  const t = new TouchController(input);
  t.stickDown(1, 0, 0);
  t.stickMove(1, 0, -R);
  assert.equal(input.sample().buttons, 0, 'touch controls are off (the menu is open)');
  input.setTouch(true);
  t.releaseAll(); // what hiding the controls does
  assert.equal(input.sample().buttons, 0);
  t.stickDown(1, 0, 0);
  t.stickMove(1, 0, -R);
  assert.equal(input.sample().buttons, BTN.FORWARD);
  input.setTouch(false);
  assert.equal(input.sample().buttons, 0);
  input.setTouch(true);
  t.lookDown(5, 0, 5000);
  t.lookMove(5, 0, -100000);
  assert.equal(input.sample().pitch, PITCH_LIMIT);
});

test('a touch player and a keyboard player make the same input, byte for byte on the wire', () => {
  const touch = makeInput();
  touch.setTouch(true);
  const t = new TouchController(touch);
  t.stickDown(1, 0, 0);
  t.stickMove(1, 0, -R);
  t.lookDown(2, 0, 0);
  t.lookMove(2, -37, 11);
  const fromTouch = touch.sample();
  // The keyboard player presses W and moves the mouse by the same yaw/pitch.
  const keys = makeInput();
  keys.locked = true;
  keys.buttons = BTN.FORWARD;
  keys.addLook(-37, 11, TOUCH_LOOK_SCALE);
  const fromKeys = keys.sample();
  assert.deepEqual(fromTouch, fromKeys);
  assert.deepEqual(sanitiseInput(fromTouch), fromTouch, 'already in the legal integer ranges');
  const wire = decode(encodeInput([{ seq: 1, input: fromTouch }]));
  assert.ok(wire && wire.type !== undefined);
});
