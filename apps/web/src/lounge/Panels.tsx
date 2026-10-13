import { Avatar, EmptyState } from '@sideroom/ui';
import { useDms, usePresence, useUsers } from './data';
import { FriendsPage } from './shell/FriendsPage';
import { MessageRequestsPage } from './shell/MessageRequestsPage';
import { useLoungeUi } from './store';

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

/** Home with no conversation selected: the Friends page, or Message Requests when chosen in the sidebar. */
export function HomeFriends() {
  const view = useLoungeUi((s) => s.homeView);
  return view === 'requests' ? <MessageRequestsPage /> : <FriendsPage />;
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
