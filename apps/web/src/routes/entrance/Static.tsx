import { EntranceShell } from '../../components/EntranceShell';
import { useT } from '../../i18n/react';

export function NoInvitation() {
  const t = useT();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>{t('entrance.static.noInvitation.title')}</h1>
        <p>{t('entrance.static.noInvitation.body')}</p>
      </main>
    </EntranceShell>
  );
}

/** Operator-editable markdown will be fetched here later (ENT-UI-010); defaults for now. */
export function HouseNotes() {
  const t = useT();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>{t('entrance.static.houseNotes.title')}</h1>
        <p>{t('entrance.static.houseNotes.body')}</p>
      </main>
    </EntranceShell>
  );
}
