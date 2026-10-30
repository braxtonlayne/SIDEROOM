import { useCallback, useEffect, useRef, useState } from 'react';
import { getToolsEngine, type ToolsEngine } from './engine';

/** The wasm engine, loaded on first use. `failed` is true when it cannot be loaded (offline, blocked). */
export function useToolsEngine(): { engine: ToolsEngine | null; failed: boolean } {
  const [engine, setEngine] = useState<ToolsEngine | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    getToolsEngine().then(
      (e) => live && setEngine(e),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, []);
  return { engine, failed };
}

/** Per-viewer convenience storage: every access is guarded (private windows and blocked storage throw). */
export function readStored<T>(key: string, fallback: T, valid: (v: unknown) => v is T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const v: unknown = JSON.parse(raw);
    return valid(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

export function writeStored(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // storage is a convenience only
  }
}

/** Copies text; reports `copied` for two seconds. Does nothing when the clipboard is blocked. */
export function useCopy(): { copied: string | null; copy: (key: string, text: string) => void } {
  const [copied, setCopied] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const copy = useCallback((key: string, text: string) => {
    const done = () => {
      setCopied(key);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(null), 2000);
    };
    try {
      void navigator.clipboard?.writeText(text).then(done, () => undefined);
    } catch {
      // clipboard blocked: nothing to do
    }
  }, []);
  return { copied, copy };
}
