import { KeybindRow } from '@sideroom/ui';
import { displayKey, SHORTCUTS } from '../../../lounge/shortcuts';
import { KEYBIND_ACTIONS, keybindConflict, useVoiceSettings } from '../../../voice/settings';
import { KeybindRecorder } from '../KeybindRecorder';
import { Card, SectionTitle } from '../parts';

/**
 * LNG-USET-053: the built-in shortcuts (read-only) and the voice keybinds you can record. Voice keybinds
 * work while the tab is focused; push to talk is only active in a call and when it is your input mode.
 */
export function KeybindsSection() {
  const groups = Array.from(new Set(SHORTCUTS.map((s) => s.group)));
  const binds = useVoiceSettings((s) => s.settings.keybinds);
  const setKeybind = useVoiceSettings((s) => s.setKeybind);
  return (
    <>
      <SectionTitle>Keybinds</SectionTitle>
      <p className="us-muted">
        The built-in shortcuts are fixed. Voice keybinds below are yours to choose.
      </p>
      <Card
        id="us-keys-voice"
        title="Voice"
        description="These work while this tab is focused. Pressing one while typing in a message box does not type it during a call."
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
                value={binds[a.id]}
                onChange={(b) => setKeybind(a.id, b)}
                conflict={(b) => keybindConflict(binds, a.id, b)}
              />
            </li>
          ))}
        </ul>
      </Card>
      {groups.map((g) => (
        <Card key={g} id={`us-keys-${g.toLowerCase()}`} title={g}>
          <dl className="us-keys">
            {SHORTCUTS.filter((s) => s.group === g).map((s) => (
              <KeybindRow key={s.id} action={s.description} keys={s.keys.map(displayKey)} />
            ))}
          </dl>
        </Card>
      ))}
    </>
  );
}
