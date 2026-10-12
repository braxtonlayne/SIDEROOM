import { KeybindRow, Notice, Select, Switch } from '@sideroom/ui';
import { displayKey, SHORTCUTS } from '../../../lounge/shortcuts';
import { Card, SectionTitle } from '../parts';
import type { PrefsFormProps } from './Appearance';

export function KeybindsSection() {
  const groups = Array.from(new Set(SHORTCUTS.map((s) => s.group)));
  return (
    <>
      <SectionTitle>Keybinds</SectionTitle>
      <p className="us-muted">These are the defaults. Custom keybinds aren&apos;t available yet.</p>
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

export function LanguageSection() {
  return (
    <>
      <SectionTitle>Language</SectionTitle>
      <Card id="us-lang" title="Language">
        <label className="us-muted" id="us-lang-label">
          Interface language
        </label>
        <Select
          aria-labelledby="us-lang-label"
          options={[{ value: 'en', label: 'English' }]}
          value="en"
          disabled
        />
        <Notice variant="info" className="us-gap">
          English is the only language for now. More are planned.
        </Notice>
      </Card>
    </>
  );
}

export function AdvancedSection({ prefs, setPrefs }: PrefsFormProps) {
  return (
    <>
      <SectionTitle>Advanced</SectionTitle>
      <Card id="us-dev" title="Developer mode">
        <Switch
          label="Developer mode"
          description="Shows extra tools such as Copy ID in menus."
          checked={prefs.developerMode}
          onChange={(e) => setPrefs({ developerMode: e.target.checked })}
        />
      </Card>
    </>
  );
}
