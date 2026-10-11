import { EntranceShell } from '../../components/EntranceShell';

export function NoInvitation() {
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>No invitation?</h1>
        <p>SIDE ROOM is invitation-only. Ask someone you know who’s a member.</p>
      </main>
    </EntranceShell>
  );
}

/** Operator-editable markdown will be fetched here later (ENT-UI-010); defaults for now. */
export function HouseNotes() {
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>House notes</h1>
        <p>
          SIDE ROOM is a small, private place for people who know each other. Be kind, keep what’s
          said here here, and look after the room.
        </p>
      </main>
    </EntranceShell>
  );
}
