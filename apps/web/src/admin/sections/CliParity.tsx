import { useT, type MessageKey } from '../../i18n';
import { SectionHeader } from '../ui';

const COMMANDS: { title: MessageKey; cmd: string; note: MessageKey }[] = [
  {
    title: 'admin.cliParity.emergencyInvite.title',
    cmd: 'sr-cli invite create --issuer <username> --note "why"',
    note: 'admin.cliParity.emergencyInvite.note',
  },
  {
    title: 'admin.cliParity.listStaff.title',
    cmd: 'sr-cli staff list',
    note: 'admin.cliParity.listStaff.note',
  },
  {
    title: 'admin.cliParity.appoint.title',
    cmd: 'sr-cli staff appoint <user> --role "Moderator" --reason "why"\nsr-cli staff revoke <user> --reason "why"',
    note: 'admin.cliParity.appoint.note',
  },
  {
    title: 'admin.cliParity.transfer.title',
    cmd: 'sr-cli staff transfer-owner <user> --reason "why"',
    note: 'admin.cliParity.transfer.note',
  },
  {
    title: 'admin.cliParity.verify.title',
    cmd: 'sr-cli verify-access-log',
    note: 'admin.cliParity.verify.note',
  },
];

/** Operator escape hatches that exist only on the command line (spec: CLI parity). Read-only information. */
export function CliParitySection() {
  const t = useT();
  return (
    <>
      <SectionHeader
        title={t('admin.cliParity.title')}
        description={t('admin.cliParity.description')}
      />
      <div className="ss-form" style={{ maxWidth: 760 }}>
        {COMMANDS.map((c) => (
          <section key={c.title} className="ss-card">
            <h2>{t(c.title)}</h2>
            <pre className="adm-pre">
              <code>{c.cmd}</code>
            </pre>
            <p className="ss-sub">{t(c.note)}</p>
          </section>
        ))}
      </div>
    </>
  );
}
