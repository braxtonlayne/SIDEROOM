import { KbdCombo, Modal } from '@sideroom/ui';
import { displayKey, SHORTCUTS } from './shortcuts';

export function ShortcutSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const groups = Array.from(new Set(SHORTCUTS.map((s) => s.group)));
  return (
    <Modal open={open} onClose={onClose} title="Keyboard shortcuts" size="md">
      {groups.map((g) => (
        <section key={g} aria-label={g}>
          <h3 className="lng-sheet-h">{g}</h3>
          <dl className="lng-sheet">
            {SHORTCUTS.filter((s) => s.group === g).map((s) => (
              <div key={s.id} className="lng-sheet-row">
                <dt>{s.description}</dt>
                <dd>
                  <KbdCombo keys={s.keys.map(displayKey)} />
                </dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </Modal>
  );
}
