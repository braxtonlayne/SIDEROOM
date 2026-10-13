import { SectionHeader } from '../ui';

const COMMANDS: { title: string; cmd: string; note: string }[] = [
  {
    title: 'Emergency invite',
    cmd: 'sr-cli invite create --issuer <username> --note "why"',
    note: 'Creates an open invite as an existing account, bypassing credits and the invite freeze. The code is printed once; it is also listed under Invites with an emergency tag.',
  },
  {
    title: 'List staff',
    cmd: 'sr-cli staff list',
    note: 'Works even when nobody can sign in to the web app.',
  },
  {
    title: 'Appoint or remove staff',
    cmd: 'sr-cli staff appoint <user> --role "Moderator" --reason "why"\nsr-cli staff revoke <user> --reason "why"',
    note: 'Same rules as the Staff page, and the same audit trail.',
  },
  {
    title: 'Transfer ownership',
    cmd: 'sr-cli staff transfer-owner <user> --reason "why"',
    note: 'The only way to move the Owner role. It is deliberately not available in the web app.',
  },
  {
    title: 'Verify the access log',
    cmd: 'sr-cli verify-access-log',
    note: 'The same hash-chain check as the Verify chain button on the Access log page.',
  },
];

/** Operator escape hatches that exist only on the command line (spec: CLI parity). Read-only information. */
export function CliParitySection() {
  return (
    <>
      <SectionHeader
        title="Command line"
        description="Everything here runs on the server with sr-cli, using the server's own configuration. Nothing on this page sends a request."
      />
      <div className="ss-form" style={{ maxWidth: 760 }}>
        {COMMANDS.map((c) => (
          <section key={c.title} className="ss-card">
            <h2>{c.title}</h2>
            <pre className="adm-pre">
              <code>{c.cmd}</code>
            </pre>
            <p className="ss-sub">{c.note}</p>
          </section>
        ))}
      </div>
    </>
  );
}
