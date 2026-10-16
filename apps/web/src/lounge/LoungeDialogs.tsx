/**
 * Host for the server-flow dialogs. Any menu opens one through `useLoungeUi().openDialog(...)`; the shell
 * renders this once.
 */
import { lazy, Suspense } from 'react';
import { AddServerDialog } from './AddServerDialog';
import { CreateChannelDialog } from './CreateChannelDialog';
import { CreateEventDialog } from './events/CreateEventDialog';
import { EventsDialog } from './events/EventsDialog';
import { InviteDialog } from './InviteDialog';
import { GuildProfileDialog } from './shell/GuildProfileDialog';
import { DirectoryDialog } from './shell/DirectoryDialog';
import { CreateDmDialog } from './shell/CreateDmDialog';
import { CustomStatusDialog } from './shell/CustomStatusDialog';
import { DeleteChannelDialog } from './shell/DeleteChannelDialog';
import { EditChannelDialog } from './shell/EditChannelDialog';
import { FolderDialog } from './shell/FolderDialog';
import { MemberActionDialog } from './shell/MemberDialogs';
import { NicknameDialog, NoteDialog } from './shell/UserDialogs';
import { LeaveServerDialog } from './shell/LeaveServerDialog';
import { NotificationDialog } from './shell/NotificationDialog';
import { BrowseChannelsDialog } from './shell/BrowseChannelsDialog';
import { SecurityActionsDialog } from './shell/SecurityActionsDialog';
import { ServerGuideDialog } from './shell/ServerGuideDialog';
import { SlowmodeDialog } from './shell/SlowmodeDialog';
import { TransferOwnershipDialog } from './shell/TransferOwnershipDialog';
import { CommunityDialog } from './community/CommunityDialog';
import { CreateThreadDialog } from './threads/CreateThreadDialog';
import { useLoungeUi } from './store';
import './shell/shell.css';

const ActivityLauncher = lazy(() =>
  import('../activities/ActivityLauncher').then((m) => ({ default: m.ActivityLauncher })),
);

export function LoungeDialogs() {
  const dialog = useLoungeUi((s) => s.dialog);
  const close = useLoungeUi((s) => s.closeDialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case 'community':
      return <CommunityDialog guildId={dialog.guildId} tab={dialog.tab} onClose={close} />;
    case 'addServer':
      return <AddServerDialog onClose={close} />;
    case 'guildProfile':
      return <GuildProfileDialog guildId={dialog.guildId} onClose={close} />;
    case 'serverGuide':
      return <ServerGuideDialog guildId={dialog.guildId} onClose={close} />;
    case 'browseChannels':
      return <BrowseChannelsDialog guildId={dialog.guildId} onClose={close} />;
    case 'securityActions':
      return <SecurityActionsDialog guildId={dialog.guildId} onClose={close} />;
    case 'directory':
      return <DirectoryDialog onClose={close} />;
    case 'createChannel':
      return (
        <CreateChannelDialog
          guildId={dialog.guildId}
          parentId={dialog.parentId}
          category={dialog.category}
          onClose={close}
        />
      );
    case 'events':
      return <EventsDialog guildId={dialog.guildId} eventId={dialog.eventId} onClose={close} />;
    case 'createEvent':
      return (
        <CreateEventDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />
      );
    case 'invite':
      return <InviteDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />;
    case 'editChannel':
      return (
        <EditChannelDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />
      );
    case 'deleteChannel':
      return (
        <DeleteChannelDialog
          guildId={dialog.guildId}
          channelId={dialog.channelId}
          onClose={close}
        />
      );
    case 'slowmode':
      return (
        <SlowmodeDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />
      );
    case 'memberAction':
      return (
        <MemberActionDialog
          action={dialog.action}
          guildId={dialog.guildId}
          userId={dialog.userId}
          onClose={close}
        />
      );
    case 'note':
      return <NoteDialog userId={dialog.userId} name={dialog.name} onClose={close} />;
    case 'nickname':
      return <NicknameDialog guildId={dialog.guildId} userId={dialog.userId} onClose={close} />;
    case 'customStatus':
      return <CustomStatusDialog onClose={close} />;
    case 'addToDm':
      return <CreateDmDialog onClose={close} addTo={dialog.channelId} />;
    case 'createDm':
      return <CreateDmDialog onClose={close} />;
    case 'folder':
      return <FolderDialog folderId={dialog.folderId} onClose={close} />;
    case 'leaveServer':
      return <LeaveServerDialog guildId={dialog.guildId} onClose={close} />;
    case 'notifications':
      return (
        <NotificationDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />
      );
    case 'transferOwnership':
      return (
        <TransferOwnershipDialog guildId={dialog.guildId} userId={dialog.userId} onClose={close} />
      );
    case 'startActivity':
      return (
        <Suspense fallback={null}>
          <ActivityLauncher open onClose={close} channelId={dialog.channelId} />
        </Suspense>
      );
    case 'createThread':
      return (
        <CreateThreadDialog
          guildId={dialog.guildId}
          channelId={dialog.channelId}
          messageId={dialog.messageId}
          onClose={close}
        />
      );
  }
}
