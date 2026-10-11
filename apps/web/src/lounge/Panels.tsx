import { Avatar, EmptyState, Icon } from '@sideroom/ui';
import { useDms, usePresence, useUsers } from './data';

/** Right-hand profile panel for a DM (LNG-DM-006 is a later item; this is the minimum useful card). */
export function DmProfilePanel({ dmId }: { dmId: string }) {
  const dm = useDms().find((d) => d.id === dmId);
  const users = useUsers();
  const user = dm ? users[dm.recipientIds[0] ?? ''] : undefined;
  const presence = usePresence(user?.id ?? '');
  if (!dm || !user) return null;
  if (dm.recipientIds.length > 1) {
    return (
      <div className="lng-profile" aria-label="Group members">
        <h2 className="lng-profile-h">Members — {dm.recipientIds.length + 1}</h2>
        <ul className="lng-profile-members">
          {dm.recipientIds.map((id) => (
            <li key={id}>
              <Avatar name={users[id]?.displayName ?? '?'} size={32} />
              <span>{users[id]?.displayName}</span>
            </li>
          ))}
        </ul>
      </div>
    );
  }
  return (
    <div className="lng-profile" aria-label={`${user.displayName} profile`}>
      <div className="lng-profile-banner" />
      <div className="lng-profile-avatar">
        <Avatar name={user.displayName} size={80} status={presence.status} />
      </div>
      <div className="lng-profile-card">
        <h2>{user.displayName}</h2>
        <p className="lng-muted">{user.username}</p>
        {presence.activity && <p>{presence.activity.text}</p>}
      </div>
    </div>
  );
}

/** Home with no conversation selected. The Friends page (LNG-FRD) replaces this. */
export function HomeFriends() {
  return (
    <section className="lng-channel-view" aria-label="Friends">
      <header className="lng-header">
        <Icon name="users" size={24} />
        <h1 className="lng-header-title">Friends</h1>
      </header>
      <EmptyState
        title="Wumpus is waiting on friends. You don't have to be!"
        description="The Friends page (Online, All, Pending, Add Friend) is built under LNG-FRD."
      />
    </section>
  );
}

export function PickChannel() {
  return (
    <section className="lng-channel-view" aria-label="No channel selected">
      <EmptyState
        title="Pick a channel"
        description="Choose a channel from the list to start talking."
      />
    </section>
  );
}
