import { Button, KbdCombo } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { keybindFromEvent, keybindParts, isModifierCode, type Keybind } from '../../voice/keybind';
import { suspendKeybinds } from '../../voice/ptt';

export interface KeybindRecorderProps {
  /** What the bind does, e.g. "Push to Talk" (names the buttons). */
  action: string;
  value: Keybind | null;
  onChange: (bind: Keybind | null) => void;
  /** A message when the bind is taken by another action; the bind is then not accepted. */
  conflict?: (bind: Keybind) => string | null;
}

/**
 * Records one key or chord (LNG-USET-044/053). While recording, every voice keybind is suspended so pressing
 * the key does not trigger it, and the key press is swallowed. Escape cancels; a lone modifier (Shift, Ctrl)
 * is accepted when it is released without another key.
 */
export function KeybindRecorder({ action, value, onChange, conflict }: KeybindRecorderProps) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const latest = useRef({ onChange, conflict });
  latest.current = { onChange, conflict };

  useEffect(() => {
    if (!recording) return;
    const resume = suspendKeybinds();
    let pendingModifier: Keybind | null = null;
    const commit = (bind: Keybind) => {
      const taken = latest.current.conflict?.(bind) ?? null;
      setProblem(taken);
      if (!taken) latest.current.onChange(bind);
      setRecording(false);
    };
    const onDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === 'Escape') {
        setRecording(false);
        return;
      }
      const bind = keybindFromEvent(e);
      if (!bind) return;
      if (isModifierCode(bind.code)) {
        pendingModifier = bind;
        return;
      }
      pendingModifier = null;
      commit(bind);
    };
    const onUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (pendingModifier && pendingModifier.code === e.code) commit(pendingModifier);
    };
    window.addEventListener('keydown', onDown, true);
    window.addEventListener('keyup', onUp, true);
    return () => {
      window.removeEventListener('keydown', onDown, true);
      window.removeEventListener('keyup', onUp, true);
      resume();
    };
  }, [recording]);

  return (
    <div className="us-keybind">
      <div className="us-keybind-value">
        {recording ? (
          <span className="us-muted" role="status">
            Press a key or combination, or Esc to cancel
          </span>
        ) : value ? (
          <KbdCombo keys={keybindParts(value)} />
        ) : (
          <span className="us-muted">Not bound</span>
        )}
      </div>
      <Button
        size="sm"
        variant="secondary"
        aria-label={`${recording ? 'Cancel recording' : value ? 'Change' : 'Record'} ${action} keybind`}
        onClick={() => {
          setProblem(null);
          setRecording((r) => !r);
        }}
      >
        {recording ? 'Cancel' : value ? 'Change' : 'Record'}
      </Button>
      {value && !recording && (
        <Button
          size="sm"
          variant="ghost"
          aria-label={`Clear ${action} keybind`}
          onClick={() => {
            setProblem(null);
            onChange(null);
          }}
        >
          Clear
        </Button>
      )}
      {problem && (
        <p className="us-keybind-problem" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
