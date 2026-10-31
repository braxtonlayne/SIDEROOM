// ACT-EMU-001: one running game. Drives the RetroArch exports of an EmulatorJS core module (emu-ops handoff section
// 1.2): the ROM, the BIOS and the save RAM are written into the module's in-memory file system, `callMain` starts
// RetroArch, and `simulate_input` / `save_state_info` / `load_state` / `cmd_savefiles` do the rest. The core's own
// Web Audio driver plays the sound. No IDBFS is mounted: save data leaves the page only through saves.ts.
import { retroIndex, type Button } from './gamepad';
import type { CoreModule } from './loader';

/** The content name inside the module. Save RAM is `<stem>.srm` in the save directory. */
export const CONTENT_STEM = 'game';
export const SAVE_DIR = '/data/saves';
const CONFIG_PATH = '/home/web_user/.config/retroarch/retroarch.cfg';
const SYSTEM_DIRS = ['/', '/home/web_user/retroarch/userdata/system'];

/** The RetroArch configuration written before start. Plain values only. */
export const RETROARCH_CFG = [
  'autosave_interval = 60',
  'screenshot_directory = "/"',
  'block_sram_overwrite = false',
  'video_gpu_screenshot = false',
  'audio_latency = 64',
  'video_vsync = true',
  'video_smooth = false',
  `savefile_directory = "${SAVE_DIR}"`,
  'system_directory = "/home/web_user/retroarch/userdata/system"',
  '',
].join('\n');

export interface StartOptions {
  /** The ROM's file name; its extension is kept (`game.<ext>`). */
  romName: string;
  rom: Uint8Array;
  /** Files that belong to the ROM (the tracks of a `.cue`), written under their own names. */
  extra?: ReadonlyArray<{ name: string; bytes: Uint8Array }>;
  /** A checked PS1 BIOS (see bios.ts), written under the file name the core looks for. In memory only. */
  bios?: { file: string; bytes: Uint8Array };
  /** Save RAM from the member's mount. */
  sram?: Uint8Array | null;
}

export const contentFileName = (romName: string): string => {
  const i = romName.lastIndexOf('.');
  const ext =
    i > 0
      ? romName
          .slice(i + 1)
          .toLowerCase()
          .replace(/[^a-z0-9]/g, '')
      : 'bin';
  return `${CONTENT_STEM}.${ext || 'bin'}`;
};

function mkdirs(m: CoreModule, dir: string) {
  let cur = '';
  for (const part of dir.split('/').filter(Boolean)) {
    cur += `/${part}`;
    if (!m.FS.analyzePath(cur).exists) m.FS.mkdir(cur);
  }
}

export class EmuSession {
  private fns = new Map<string, (...a: unknown[]) => unknown>();
  private running = false;
  private contentPath = '';

  constructor(private readonly m: CoreModule) {}

  private fn(name: string, ret: string | null, args: string[]) {
    let f = this.fns.get(name);
    if (!f) {
      f = this.m.cwrap(name, ret, args);
      this.fns.set(name, f);
    }
    return f;
  }

  /** Write everything the core needs and start the main loop. */
  start(o: StartOptions): void {
    const m = this.m;
    mkdirs(m, '/home/web_user/.config/retroarch');
    mkdirs(m, SAVE_DIR);
    for (const d of SYSTEM_DIRS) mkdirs(m, d);
    m.FS.writeFile(CONFIG_PATH, RETROARCH_CFG);
    const name = contentFileName(o.romName);
    this.contentPath = `/${name}`;
    m.FS.writeFile(this.contentPath, o.rom);
    for (const x of o.extra ?? []) {
      if (/[/\\]/.test(x.name)) continue;
      m.FS.writeFile(`/${x.name}`, x.bytes);
    }
    if (o.bios) {
      for (const d of SYSTEM_DIRS) {
        m.FS.writeFile(`${d === '/' ? '' : d}/${o.bios.file}`, o.bios.bytes);
      }
    }
    if (o.sram && o.sram.length > 0) m.FS.writeFile(`${SAVE_DIR}/${CONTENT_STEM}.srm`, o.sram);
    m.callMain([this.contentPath]);
    m.resumeMainLoop();
    this.running = true;
    try {
      // The shell owns the keyboard: the core must not also listen to it.
      (this.fn('ejs_set_keyboard_enabled', null, ['number']) as (v: number) => void)(0);
    } catch {
      /* an older build without the export */
    }
  }

  get isRunning(): boolean {
    return this.running;
  }

  /** Press or release a RetroPad button of a player (0 to 3). */
  setButton(player: number, button: Button, pressed: boolean): void {
    if (!this.running) return;
    (
      this.fn('simulate_input', null, ['number', 'number', 'number']) as (
        p: number,
        i: number,
        v: number,
      ) => void
    )(player, retroIndex(button), pressed ? 1 : 0);
  }

  get supportsStates(): boolean {
    try {
      return Number(this.fn('supports_states', 'number', [])()) !== 0;
    } catch {
      return false;
    }
  }

  /** A save state: `save_state_info` answers `size|pointer|ok`, the bytes are read out of the module's heap. */
  saveState(): Uint8Array {
    const info = String(this.fn('save_state_info', 'string', [])()).split('|');
    if (info[2] !== '1') throw new Error(info[0] || 'the core cannot save a state');
    const size = parseInt(info[0] ?? '', 10);
    const at = parseInt(info[1] ?? '', 10);
    if (!Number.isFinite(size) || !Number.isFinite(at) || size <= 0) throw new Error('no state');
    return new Uint8Array(this.m.HEAPU8.subarray(at, at + size));
  }

  loadState(bytes: Uint8Array): void {
    try {
      this.m.FS.unlink('/game.state');
    } catch {
      /* none yet */
    }
    this.m.FS.writeFile('/game.state', bytes);
    (this.fn('load_state', 'number', ['string', 'number']) as (p: string, n: number) => void)(
      'game.state',
      0,
    );
  }

  /** Flush the core's save RAM to its file and read it; `null` when the game has none. */
  readSram(): Uint8Array | null {
    try {
      (this.fn('cmd_savefiles', '', []) as () => void)();
      const path = String(
        this.fn('save_file_path', 'string', [])() || `${SAVE_DIR}/${CONTENT_STEM}.srm`,
      );
      if (!this.m.FS.analyzePath(path).exists) return null;
      const data = this.m.FS.readFile(path);
      return data.length > 0 ? data : null;
    } catch {
      return null;
    }
  }

  pause(): void {
    try {
      (this.fn('toggleMainLoop', null, ['number']) as (v: number) => void)(0);
    } catch {
      this.m.pauseMainLoop?.();
    }
  }

  resume(): void {
    try {
      (this.fn('toggleMainLoop', null, ['number']) as (v: number) => void)(1);
    } catch {
      this.m.resumeMainLoop();
    }
  }

  restart(): void {
    (this.fn('system_restart', '', []) as () => void)();
  }

  /** The number of frames the core has run (the e2e waits for it to move). */
  frameCount(): number {
    try {
      return Number(this.fn('get_current_frame_count', 'number', [''])());
    } catch {
      return 0;
    }
  }

  /** Stop the loop and drop the module. The BIOS and the ROM go with it: nothing was written anywhere else. */
  dispose(): void {
    if (!this.running) return;
    this.running = false;
    try {
      this.pause();
    } catch {
      /* already gone */
    }
    try {
      this.m.abort?.();
    } catch {
      /* Emscripten aborts by throwing */
    }
  }
}
