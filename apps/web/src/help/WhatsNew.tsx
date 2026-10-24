import { Link } from '@tanstack/react-router';
import { useEffect } from 'react';
import { EntranceShell } from '../components/EntranceShell';
import { useT } from '../i18n/react';
import { markNewsSeen } from '../lib/whatsNew';
import changelog from '../../../../CHANGELOG.md?raw';
import { EntranceMarkdown } from '../routes/entrance/entranceCopy';

/**
 * The part of CHANGELOG.md the page shows: from the first `##` heading on (the lines above are a note to the
 * integrator), with wrapped list items joined back into one line for the small renderer.
 */
export function changelogBody(source: string): string {
  const from = source.search(/^## /m);
  const body = from >= 0 ? source.slice(from) : '';
  return body.replace(/\r\n?/g, '\n').replace(/\n {2,}(?![-*]\s)(?=\S)/g, ' ');
}

/** `/help/whats-new` (RDY-ONB-003): the changelog, newest wave first. Opening it clears the badge on the help button. */
export function WhatsNew() {
  const t = useT();
  useEffect(() => markNewsSeen(), []);
  const body = changelogBody(changelog);
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes ent-changelog">
        <h1>{t('help.whatsNew.title')}</h1>
        <p>{t('help.whatsNew.intro')}</p>
        {body ? <EntranceMarkdown source={body} /> : <p>{t('help.whatsNew.empty')}</p>}
        <p>
          <Link to="/help">{t('help.whatsNew.back')}</Link>
        </p>
      </main>
    </EntranceShell>
  );
}
