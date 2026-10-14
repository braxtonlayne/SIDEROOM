import { Button, KbdCombo } from '@sideroom/ui';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  comboFromEvent,
  comboKeys,
  defaultCombo,
  effectiveCombo,
  isRebindable,
  SHORTCUTS,
  type ShortcutDef,
} from '../../../lounge/shortcuts';
import { t, useT } from '../../../i18n';
import { KEYBIND_ACTIONS, keybindConflict, useVoiceSettings } from '../../../voice/settings';
import { KeybindRecorder } from '../KeybindRecorder';
import { Card, SectionTitle } from '../parts';
import type { PrefsFormProps } from './Appearance';

/** Why a recorded combo cannot be used, or null. */
export function comboProblem(
  combo: string,
  id: string,
  overrides: Record<string, string>,
): string | null {
  const hasModifier = /^(Mod|Alt)\+/.test(combo);
  const special =
    /^(Shift\+)?(F\d{1,2}|Arrow\w+|Tab|Enter|Escape|Backspace|Delete|Home|End|Page\w+)$/;
  // A bare letter would be typed into the message box (and Escape closes dialogs).
  if (!hasModifier && !special.test(combo)) return t('settings.keybinds.needModifier');
  if (/^(Shift\+)?Escape$/.test(combo)) return t('settings.keybinds.escapeReserved');
  const other = SHORTCUTS.find(
    (d) => d.id !== id && isRebindable(d.id) && effectiveCombo(d, overrides) === combo,
  );
  return other ? t('settings.keybinds.alreadyUsed', { description: other.description }) : null;
}

function Row({ def, prefs, setPrefs }: PrefsFormProps & { def: ShortcutDef }) {
  const t = useT();
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const capture = useRef<HTMLButtonElement>(null);
  const combo = effectiveCombo(def, prefs.keybinds);
  const custom = def.id in prefs.keybinds;

  useEffect(() => {
    if (recording) capture.current?.focus();
  }, [recording]);

  const set = (value: string | null) => {
    const next = { ...prefs.keybinds };
    if (value === null || value === defaultCombo(def)) delete next[def.id];
    else next[def.id] = value;
    setPrefs({ keybinds: next });
  };

  const onKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      setRecording(false);
      setError(null);
      return;
    }
    const c = comboFromEvent(e.nativeEvent);
    e.preventDefault();
    e.stopPropagation();
    if (!c) return; // a modifier alone: wait for the key
    const problem = comboProblem(c, def.id, prefs.keybinds);
    if (problem) return setError(problem);
    set(c);
    setError(null);
    setRecording(false);
  };

  if (!isRebindable(def.id))
    return (
      <div className="us-key-row">
        <dt>{def.description}</dt>
        <dd>
          <KbdCombo keys={def.keys} />
          <span className="us-muted"> {t('settings.keybinds.fixed')}</span>
        </dd>
      </div>
    );

  return (
    <div className="us-key-row">
      <dt>{def.description}</dt>
      <dd>
        {recording ? (
          <>
            <button
              ref={capture}
              type="button"
              className="us-key-capture"
              aria-label={t('settings.keybinds.pressAria', { description: def.description })}
              onKeyDown={onKey}
              onBlur={() => {
                setRecording(false);
                setError(null);
              }}
            >
              {t('settings.keybinds.pressKeys')}
            </button>
            {error && (
              <span role="alert" className="us-key-error">
                {error}
              </span>
            )}
          </>
        ) : (
          <>
            {combo === '' ? (
              <span className="us-muted">{t('settings.keybinds.notBound')}</span>
            ) : (
              <KbdCombo keys={comboKeys(combo)} />
            )}
            <span className="us-actions">
              <Button variant="secondary" size="sm" onClick={() => setRecording(true)}>
                {t('settings.keybinds.change')}
                <span className="us-sr"> {def.description}</span>
              </Button>
              {combo !== '' && (
                <Button variant="ghost" size="sm" onClick={() => set('')}>
                  {t('settings.keybinds.unbind')}
                  <span className="us-sr"> {def.description}</span>
                </Button>
              )}
              {custom && (
                <Button variant="ghost" size="sm" onClick={() => set(null)}>
                  {t('settings.keybinds.reset')}
                  <span className="us-sr"> {def.description}</span>
                </Button>
              )}
            </span>
          </>
        )}
      </dd>
    </div>
  );
}

export function KeybindsSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const groups = Array.from(new Set(SHORTCUTS.map((s) => s.group)));
  const customCount = Object.keys(prefs.keybinds).length;
  const voiceBinds = useVoiceSettings((s) => s.settings.keybinds);
  const setVoiceKeybind = useVoiceSettings((s) => s.setKeybind);
  return (
    <>
      <SectionTitle>{t('settings.keybinds.title')}</SectionTitle>
      <p className="us-muted">{t('settings.keybinds.intro')}</p>
      {customCount > 0 && (
        <p>
          <Button variant="link" size="sm" onClick={() => setPrefs({ keybinds: {} })}>
            {t('settings.keybinds.resetAll')}
          </Button>
        </p>
      )}
      <Card
        id="us-keys-voice"
        title={t('settings.keybinds.voiceTitle')}
        description={t('settings.keybinds.voiceDesc')}
      >
        <ul className="us-keybind-list">
          {KEYBIND_ACTIONS.map((a) => (
            <li key={a.id} className="us-keybind-row">
              <div>
                <strong>{a.label}</strong>
                <p className="us-muted">{a.description}</p>
              </div>
              <KeybindRecorder
                action={a.label}
                value={voiceBinds[a.id]}
                onChange={(b) => setVoiceKeybind(a.id, b)}
                conflict={(b) => keybindConflict(voiceBinds, a.id, b)}
              />
            </li>
          ))}
        </ul>
      </Card>
      {groups.map((g) => (
        <Card key={g} id={`us-keys-${g.toLowerCase()}`} title={g}>
          <dl className="us-keys">
            {SHORTCUTS.filter((s) => s.group === g).map((s) => (
              <Row key={s.id} def={s} prefs={prefs} setPrefs={setPrefs} />
            ))}
          </dl>
        </Card>
      ))}
    </>
  );
}
