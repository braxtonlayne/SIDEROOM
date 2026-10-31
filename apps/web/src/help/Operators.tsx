import { Link } from '@tanstack/react-router';
import { EntranceShell } from '../components/EntranceShell';
import { useT, type MessageKey } from '../i18n';

/** The module switches an operator finds in Admin > Features: the switch id and the sentence that says what it does. */
export const OPERATOR_SWITCHES: readonly { id: string; text: MessageKey }[] = [
  { id: 'platform.byos', text: 'help.operators.byos' },
  { id: 'lounge.jukebox', text: 'help.operators.jukebox' },
  { id: 'lounge.listening_rooms', text: 'help.operators.rooms' },
  { id: 'platform.tools', text: 'help.operators.tools' },
  { id: 'platform.activities_datachannel', text: 'help.operators.datachannel' },
  { id: 'platform.emulator', text: 'help.operators.emulator' },
  { id: 'platform.watch_together_embeds', text: 'help.operators.embeds' },
  { id: 'platform.bridge', text: 'help.operators.bridge' },
  { id: 'lounge.scrapbook', text: 'help.operators.scrapbook' },
  { id: 'lounge.rings', text: 'help.operators.rings' },
  { id: 'platform.public_profiles', text: 'help.operators.profiles' },
  { id: 'platform.game_servers', text: 'help.operators.gameServers' },
];

/** `/help/operators` (OPS-DOC-001): what each module switch does, for the people who run the instance. */
export function HelpOperators() {
  const t = useT();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes">
        <h1>{t('help.operators.title')}</h1>
        <p>{t('help.operators.intro')}</p>
        <h2 id="switches">{t('help.operators.listTitle')}</h2>
        <ul>
          {OPERATOR_SWITCHES.map((s) => (
            <li key={s.id}>
              <code>{s.id}</code>: {t(s.text)}
            </li>
          ))}
        </ul>
        <h2 id="more">{t('help.operators.moreTitle')}</h2>
        <p>{t('help.operators.more')}</p>
        <p>
          <Link to="/help">{t('help.operators.back')}</Link>
        </p>
      </main>
    </EntranceShell>
  );
}
