import { KbdCombo, Modal } from '@sideroom/ui';
import { BETA_REPORTS_FLAG } from '../bugreport/flag';
import { useT } from '../i18n';
import { useClientFlag } from '../moderation/reports';
import { useSavedPrefs } from '../settings/user/prefs';
import { comboKeys, displayKey, effectiveCombo, isRebindable, SHORTCUTS } from './shortcuts';

export function ShortcutSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const { keybinds } = useSavedPrefs();
  const bugOn = useClientFlag(BETA_REPORTS_FLAG);
  const shown = SHORTCUTS.filter((s) => s.id !== 'bugReport' || bugOn);
  const groups = Array.from(new Set(shown.map((s) => s.group)));
  return (
    <Modal open={open} onClose={onClose} title={t('lounge.shortcutSheet.title')} size="md">
      {groups.map((g) => (
        <section key={g} aria-label={t(`lounge.shortcuts.group${g}`)}>
          <h3 className="lng-sheet-h">{t(`lounge.shortcuts.group${g}`)}</h3>
          <dl className="lng-sheet">
            {shown
              .filter((s) => s.group === g)
              .map((s) => (
                <div key={s.id} className="lng-sheet-row">
                  <dt>{s.description}</dt>
                  <dd>
                    <KbdCombo
                      keys={
                        isRebindable(s.id)
                          ? comboKeys(effectiveCombo(s, keybinds))
                          : s.keys.map(displayKey)
                      }
                    />
                  </dd>
                </div>
              ))}
          </dl>
        </section>
      ))}
    </Modal>
  );
}
