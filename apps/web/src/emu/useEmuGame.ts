// ACT-EMU-001: runs one game inside the player: prepare, load the core, start, poll input, keep save RAM on the
// member's mount, and clean up. The page owns the canvas; this hook owns the session.
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { Mount } from '../settings/user/storage/types';
import { coopGameFor, offerCoop, withdrawCoop, type CoopGame } from './coopOffer';
import type { CoreInfo } from './cores';
import type { PadMapping } from './gamepad';
import { InputPoller } from './input';
import { CoreUnavailableError, loadCore, realLoaderDeps } from './loader';
import { PlayError, prepareGame, sameBytes, type Prepared } from './play';
import { readObject, type RomItem } from './romApi';
import { EmuSession } from './session';
import { getSave, pickSaveMount, putSave, saveKey, type SaveName, type StateSlot } from './saves';

export type GameError =
  | { kind: 'core-missing'; core: CoreInfo | string }
  | { kind: 'webgl2' }
  | { kind: 'platform' }
  | { kind: 'bios' }
  | { kind: 'rom'; detail?: string }
  | { kind: 'failed'; detail: string };

export type GameStatus =
  | { phase: 'idle' }
  | { phase: 'loading'; stage: 'rom' | 'download' | 'unpack' | 'start'; fraction: number }
  | { phase: 'running' | 'paused' }
  | { phase: 'error'; error: GameError };

export type SaveStatus =
  | { kind: 'none' }
  | { kind: 'ready'; mountLabel: string }
  | { kind: 'saving' }
  | { kind: 'saved'; at: number; what: SaveName }
  | { kind: 'error'; message: string };

export interface GameBios {
  file: string;
  bytes: Uint8Array;
}

export interface UseEmuGame {
  status: GameStatus;
  save: SaveStatus;
  needsBios: boolean;
  pause(): void;
  resume(): void;
  restart(): void;
  saveState(slot: StateSlot): Promise<void>;
  loadState(slot: StateSlot): Promise<void>;
  /** Used by tests and the co-op seam. */
  session: EmuSession | null;
}

const SRAM_INTERVAL_MS = 30_000;

export function useEmuGame(opts: {
  rom: RomItem | null;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  mounts: readonly Mount[];
  bios: GameBios | null;
  mapping: PadMapping;
}): UseEmuGame {
  const { rom, canvasRef, mounts, bios, mapping } = opts;
  const [status, setStatus] = useState<GameStatus>({ phase: 'idle' });
  const [save, setSave] = useState<SaveStatus>({ kind: 'none' });
  const [needsBios, setNeedsBios] = useState(false);
  const [session, setSession] = useState<EmuSession | null>(null);
  const live = useRef<{
    session: EmuSession;
    prepared: Prepared;
    saveMount: Mount | null;
    lastSram: Uint8Array | null;
  } | null>(null);
  const mappingRef = useRef(mapping);
  mappingRef.current = mapping;
  const mountsRef = useRef(mounts);
  mountsRef.current = mounts;
  const biosRef = useRef(bios);
  biosRef.current = bios;

  const flushSram = useCallback(async () => {
    const g = live.current;
    if (!g?.saveMount) return;
    const sram = g.session.readSram();
    if (!sram || sameBytes(sram, g.lastSram)) return;
    try {
      await putSave(g.saveMount.id, saveKey(g.prepared.core.id, g.prepared.hash, 'sram'), sram);
      g.lastSram = sram;
      setSave({ kind: 'saved', at: Date.now(), what: 'sram' });
    } catch (e) {
      setSave({ kind: 'error', message: e instanceof Error ? e.message : 'save failed' });
    }
  }, []);

  useEffect(() => {
    if (!rom) {
      setStatus({ phase: 'idle' });
      return;
    }
    let cancelled = false;
    let teardown: (() => void) | null = null;
    const fail = (error: GameError) => !cancelled && setStatus({ phase: 'error', error });

    void (async () => {
      const saveMount = pickSaveMount(mountsRef.current);
      setSave(saveMount ? { kind: 'ready', mountLabel: saveMount.label } : { kind: 'none' });
      setStatus({ phase: 'loading', stage: 'rom', fraction: 0 });
      let prepared: Prepared;
      try {
        prepared = await prepareGame(
          rom,
          {
            read: (m, k, p) => readObject(m, k, undefined, p),
            saveMountId: saveMount?.id ?? null,
            loadSave: getSave,
          },
          (done, total) =>
            !cancelled &&
            setStatus({ phase: 'loading', stage: 'rom', fraction: total > 0 ? done / total : 0 }),
        );
      } catch (e) {
        if (e instanceof PlayError)
          fail(e.kind === 'rom' ? { kind: 'rom', detail: e.message } : { kind: e.kind });
        else fail({ kind: 'failed', detail: e instanceof Error ? e.message : String(e) });
        return;
      }
      if (cancelled) return;
      // The BIOS rule: a core that needs one starts only with a checked BIOS from the member's own mount.
      setNeedsBios(prepared.core.needsBios);
      if (prepared.core.needsBios && !biosRef.current) {
        fail({ kind: 'bios' });
        return;
      }
      const canvas = canvasRef.current;
      if (!canvas) return;
      let loaded;
      try {
        loaded = await loadCore(
          prepared.core,
          {
            canvas,
            onProgress: (stage, fraction) =>
              !cancelled && setStatus({ phase: 'loading', stage, fraction }),
          },
          realLoaderDeps(),
        );
      } catch (e) {
        if (e instanceof CoreUnavailableError) {
          fail(
            e.why === 'webgl2' ? { kind: 'webgl2' } : { kind: 'core-missing', core: prepared.core },
          );
        } else {
          fail({ kind: 'failed', detail: e instanceof Error ? e.message : String(e) });
        }
        return;
      }
      if (cancelled) return;

      const s = new EmuSession(loaded.module);
      try {
        s.start({
          romName: rom.key.split('/').pop() ?? rom.key,
          rom: prepared.rom,
          extra: prepared.extra,
          bios: biosRef.current
            ? { file: biosRef.current.file, bytes: biosRef.current.bytes }
            : undefined,
          sram: prepared.sram,
        });
      } catch (e) {
        fail({ kind: 'failed', detail: e instanceof Error ? e.message : String(e) });
        return;
      }
      live.current = { session: s, prepared, saveMount, lastSram: prepared.sram };
      setSession(s);
      setStatus({ phase: 'running' });

      // Input: the keyboard through window events, pads polled once per frame.
      const poller = new InputPoller(
        (p, b, v) => s.setButton(p, b, v),
        () => mappingRef.current,
        () => (navigator.getGamepads ? Array.from(navigator.getGamepads()) : []),
      );
      const typing = (t: EventTarget | null) =>
        t instanceof HTMLElement &&
        (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      const onKey = (down: boolean) => (ev: KeyboardEvent) => {
        if (typing(ev.target)) return;
        const used = Object.values(mappingRef.current.keyboard).includes(ev.code);
        if (used) ev.preventDefault();
        poller.key(ev.code, down);
      };
      const kd = onKey(true);
      const ku = onKey(false);
      const blur = () => poller.releaseAll();
      window.addEventListener('keydown', kd);
      window.addEventListener('keyup', ku);
      window.addEventListener('blur', blur);
      let raf = 0;
      const tick = () => {
        poller.poll();
        raf = requestAnimationFrame(tick);
      };
      raf = requestAnimationFrame(tick);
      const timer = window.setInterval(() => void flushSram(), SRAM_INTERVAL_MS);
      const onHide = () => {
        if (document.visibilityState === 'hidden') void flushSram();
      };
      document.addEventListener('visibilitychange', onHide);

      const coop: CoopGame = coopGameFor(rom.title, s);
      void offerCoop(coop);

      teardown = () => {
        cancelAnimationFrame(raf);
        window.clearInterval(timer);
        window.removeEventListener('keydown', kd);
        window.removeEventListener('keyup', ku);
        window.removeEventListener('blur', blur);
        document.removeEventListener('visibilitychange', onHide);
        void withdrawCoop(coop);
        poller.releaseAll();
      };
    })();

    return () => {
      cancelled = true;
      teardown?.();
      const g = live.current;
      live.current = null;
      if (g) {
        // Last save RAM goes out before the core is dropped; the BIOS and ROM bytes go with the module.
        const sram = g.session.readSram();
        if (g.saveMount && sram && !sameBytes(sram, g.lastSram)) {
          void putSave(
            g.saveMount.id,
            saveKey(g.prepared.core.id, g.prepared.hash, 'sram'),
            sram,
          ).catch(() => undefined);
        }
        g.session.dispose();
      }
      setSession(null);
    };
    // The game restarts only when another ROM is chosen or the BIOS arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rom?.id, bios?.file]);

  const pause = useCallback(() => {
    live.current?.session.pause();
    setStatus((s) => (s.phase === 'running' ? { phase: 'paused' } : s));
  }, []);
  const resume = useCallback(() => {
    live.current?.session.resume();
    setStatus((s) => (s.phase === 'paused' ? { phase: 'running' } : s));
  }, []);
  const restart = useCallback(() => live.current?.session.restart(), []);

  const saveState = useCallback(async (slot: StateSlot) => {
    const g = live.current;
    if (!g?.saveMount) {
      setSave({ kind: 'error', message: 'no mount' });
      return;
    }
    setSave({ kind: 'saving' });
    try {
      const name = `state${slot}` as SaveName;
      await putSave(
        g.saveMount.id,
        saveKey(g.prepared.core.id, g.prepared.hash, name),
        g.session.saveState(),
      );
      setSave({ kind: 'saved', at: Date.now(), what: name });
    } catch (e) {
      setSave({ kind: 'error', message: e instanceof Error ? e.message : 'save failed' });
    }
  }, []);

  const loadState = useCallback(async (slot: StateSlot) => {
    const g = live.current;
    if (!g?.saveMount) return;
    try {
      const bytes = await getSave(
        g.saveMount.id,
        saveKey(g.prepared.core.id, g.prepared.hash, `state${slot}` as SaveName),
      );
      if (!bytes) {
        setSave({ kind: 'error', message: 'empty' });
        return;
      }
      g.session.loadState(bytes);
    } catch (e) {
      setSave({ kind: 'error', message: e instanceof Error ? e.message : 'load failed' });
    }
  }, []);

  return { status, save, needsBios, pause, resume, restart, saveState, loadState, session };
}
