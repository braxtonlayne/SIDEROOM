/**
 * Host for the server-flow dialogs. Any menu opens one through `useLoungeUi().openDialog(...)`; the shell
 * renders this once.
 */
import { AddServerDialog } from './AddServerDialog';
import { CreateChannelDialog } from './CreateChannelDialog';
import { InviteDialog } from './InviteDialog';
import { useLoungeUi } from './store';

export function LoungeDialogs() {
  const dialog = useLoungeUi((s) => s.dialog);
  const close = useLoungeUi((s) => s.closeDialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case 'addServer':
      return <AddServerDialog onClose={close} />;
    case 'createChannel':
      return (
        <CreateChannelDialog
          guildId={dialog.guildId}
          parentId={dialog.parentId}
          category={dialog.category}
          onClose={close}
        />
      );
    case 'invite':
      return <InviteDialog guildId={dialog.guildId} channelId={dialog.channelId} onClose={close} />;
  }
}
