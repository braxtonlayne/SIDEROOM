#!/usr/bin/env node
// ACT-EMU-001: assembles the emulator's test ROM, an iNES (NROM-128) file written from scratch for this repository
// (no ROM is ever downloaded). Output: e2e/fixtures/emu/sideroom-test.nes, committed, and checked byte for byte by
// apps/web/src/emu/__tests__/testRom.test.ts, so the script and the file cannot drift apart.
//
// What it does: the whole screen is one colour (the PPU's universal background colour, no tiles needed).
//   nothing pressed          red     ($16)
//   controller 1, A held     blue    ($12)
//   controller 2, A held     green   ($2A)   (read from $4017; the co-op live case uses this one)
// Controller 2 wins when both are held.
//
// Usage: node scripts/make-test-rom.mjs [--check]   (--check exits 1 when the committed file differs)
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const COLOR_IDLE = 0x16;
export const COLOR_P1_A = 0x12;
export const COLOR_P2_A = 0x2a;

/** A two-pass 6502 assembler for the dozen instructions the ROM needs. Branch targets are labels. */
function assemble(program, origin) {
  const labels = new Map();
  const size = (it) => (it.label ? 0 : it.bytes ? it.bytes.length : 2);
  let pc = origin;
  for (const it of program) {
    if (it.label) labels.set(it.label, pc);
    pc += size(it);
  }
  const out = [];
  pc = origin;
  for (const it of program) {
    if (it.label) continue;
    if (it.bytes) {
      out.push(...it.bytes);
      pc += it.bytes.length;
    } else {
      // relative branch: opcode + displacement from the next instruction
      const target = labels.get(it.to);
      if (target === undefined) throw new Error(`unknown label ${it.to}`);
      const rel = target - (pc + 2);
      if (rel < -128 || rel > 127) throw new Error(`branch to ${it.to} out of range`);
      out.push(it.branch, rel & 0xff);
      pc += 2;
    }
  }
  return { code: out, labels };
}

const L = (label) => ({ label });
const B = (...bytes) => ({ bytes });
const BPL = (to) => ({ branch: 0x10, to });
const BEQ = (to) => ({ branch: 0xf0, to });
const lo = (a) => a & 0xff;
const hi = (a) => (a >> 8) & 0xff;
const STA_ABS = (a) => B(0x8d, lo(a), hi(a));
const LDA_ABS = (a) => B(0xad, lo(a), hi(a));
const LDA_IMM = (v) => B(0xa9, v);
const LDX_IMM = (v) => B(0xa2, v);
const STX_ABS = (a) => B(0x8e, lo(a), hi(a));
const BIT_ABS = (a) => B(0x2c, lo(a), hi(a));

const PPUCTRL = 0x2000;
const PPUMASK = 0x2001;
const PPUSTATUS = 0x2002;
const PPUSCROLL = 0x2005;
const PPUADDR = 0x2006;
const PPUDATA = 0x2007;
const JOY1 = 0x4016;
const JOY2 = 0x4017;
const ORIGIN = 0xc000;

export function buildRom() {
  const program = [
    L('reset'),
    B(0x78), // SEI
    B(0xd8), // CLD
    LDX_IMM(0x40),
    STX_ABS(JOY2), // frame counter IRQ off (this write is the APU's, not the pad's)
    LDX_IMM(0xff),
    B(0x9a), // TXS
    B(0xe8), // INX -> 0
    STX_ABS(PPUCTRL),
    STX_ABS(PPUMASK),
    STX_ABS(0x4010),
    L('vb1'),
    BIT_ABS(PPUSTATUS),
    BPL('vb1'),
    L('vb2'),
    BIT_ABS(PPUSTATUS),
    BPL('vb2'),

    L('frame'),
    // Strobe both pads, then read the first bit of each: that bit is the A button.
    LDA_IMM(0x01),
    STA_ABS(JOY1),
    LDA_IMM(0x00),
    STA_ABS(JOY1),
    LDA_ABS(JOY1),
    B(0x29, 0x01), // AND #1
    B(0x85, 0x00), // STA $00  (pad 1, A)
    LDA_ABS(JOY2),
    B(0x29, 0x01),
    B(0x85, 0x01), // STA $01  (pad 2, A)
    // Pick the colour.
    LDA_IMM(COLOR_IDLE),
    B(0xa6, 0x00), // LDX $00
    BEQ('p2'),
    LDA_IMM(COLOR_P1_A),
    L('p2'),
    B(0xa6, 0x01), // LDX $01
    BEQ('paint'),
    LDA_IMM(COLOR_P2_A),
    L('paint'),
    B(0x85, 0x02), // STA $02
    // Wait for vertical blank, then write the universal background colour.
    L('wait'),
    BIT_ABS(PPUSTATUS),
    BPL('wait'),
    LDA_IMM(0x3f),
    STA_ABS(PPUADDR),
    LDA_IMM(0x00),
    STA_ABS(PPUADDR),
    B(0xa5, 0x02), // LDA $02
    STA_ABS(PPUDATA),
    LDA_IMM(0x00),
    STA_ABS(PPUADDR),
    STA_ABS(PPUADDR),
    STA_ABS(PPUSCROLL),
    STA_ABS(PPUSCROLL),
    LDA_IMM(0x0a), // background on, left 8 pixels shown
    STA_ABS(PPUMASK),
    B(0x4c, lo(ORIGIN + 0x0), hi(ORIGIN + 0x0)), // placeholder, patched below
  ];
  // Patch the JMP to the `frame` label.
  const first = assemble(program.slice(0, -1), ORIGIN);
  const frame = first.labels.get('frame');
  program[program.length - 1] = B(0x4c, lo(frame), hi(frame));
  const { code } = assemble(program, ORIGIN);

  const prg = new Uint8Array(16 * 1024);
  prg.set(code, 0);
  // Vectors: NMI, RESET, IRQ all at the reset code (the ROM never enables NMI or IRQ).
  const vec = [ORIGIN, ORIGIN, ORIGIN];
  vec.forEach((v, i) => {
    prg[0x3ffa + i * 2] = lo(v);
    prg[0x3ffb + i * 2] = hi(v);
  });
  const header = new Uint8Array(16);
  header.set([0x4e, 0x45, 0x53, 0x1a, 1, 1, 0, 0]);
  const chr = new Uint8Array(8 * 1024); // blank tiles: the universal colour shows everywhere
  const rom = new Uint8Array(16 + prg.length + chr.length);
  rom.set(header, 0);
  rom.set(prg, 16);
  rom.set(chr, 16 + prg.length);
  return rom;
}

const here = dirname(fileURLToPath(import.meta.url));
export const OUTPUT = resolve(here, '../e2e/fixtures/emu/sideroom-test.nes');

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const rom = buildRom();
  if (process.argv.includes('--check')) {
    let have;
    try {
      have = readFileSync(OUTPUT);
    } catch {
      have = null;
    }
    if (!have || Buffer.compare(Buffer.from(rom), have) !== 0) {
      console.error(`${OUTPUT} differs from the generator output; run node scripts/make-test-rom.mjs`);
      process.exit(1);
    }
    console.log('test ROM ok');
  } else {
    mkdirSync(dirname(OUTPUT), { recursive: true });
    writeFileSync(OUTPUT, rom);
    console.log(`wrote ${OUTPUT} (${rom.length} bytes)`);
  }
}
