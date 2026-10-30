import { EntranceShell } from '../../components/EntranceShell';
import { useT } from '../../i18n/react';
import { EntranceMarkdown, useEntranceCopy } from './entranceCopy';

export function NoInvitation() {
  const t = useT();
  const custom = useEntranceCopy()['no-invitation'];
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>{t('entrance.static.noInvitation.title')}</h1>
        {custom ? (
          <EntranceMarkdown source={custom} />
        ) : (
          <p>{t('entrance.static.noInvitation.body')}</p>
        )}
      </main>
    </EntranceShell>
  );
}
