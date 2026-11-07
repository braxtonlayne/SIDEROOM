// SEC-AUD-036 / 037 (wave 54): a failed member action says so, and a failed load never offers an empty form.
export default {
  'lounge.actionError.delete': 'The message was not deleted. Try again.',
  'lounge.actionError.pin': 'The message was not pinned. Try again.',
  'lounge.actionError.unpin': 'The message was not unpinned. Try again.',
  'lounge.actionError.vote': 'Your vote was not counted. Try again.',
  'lounge.actionError.endPoll': 'The poll was not ended. Try again.',
  'lounge.actionError.edit': 'Your edit was not saved. Try again.',
  'lounge.actionError.react': 'Your reaction was not saved. Try again.',
  'lounge.actionError.forward': 'The message was not forwarded. Try again.',
  'lounge.actionError.invite': 'The invite was not sent. Try again.',
  'lounge.actionError.mute': 'Mute was not saved. Try again.',
  'lounge.actionError.unmute': 'Unmute was not saved. Try again.',
  'lounge.actionError.notify': 'That notification setting was not saved. Try again.',
  'lounge.actionError.closeDm': 'The conversation was not closed. Try again.',
  'lounge.actionError.addRole': 'The role was not added. Try again.',
  'lounge.actionError.removeRole': 'The role was not removed. Try again.',
  'lounge.actionError.openDm': 'The conversation could not be opened. Try again.',
  'lounge.actionError.remind': 'The reminder was not set. Try again.',
  'lounge.actionError.followThread': 'You are not following this thread yet. Try again.',
  'lounge.actionError.leaveThread': 'You are still following this thread. Try again.',
  'lounge.actionError.status': 'Your status was not changed. Try again.',
  'lounge.actionError.soundFavorites':
    'Your favourite sounds were not saved to your account. They stay on this device for now.',
  'lounge.actionError.loadServer': 'This server could not be loaded. Open it again in a moment.',
  'lounge.actionError.loadMembers':
    'The member list could not be loaded. Open the server again in a moment.',
  'lounge.messageList.loadFailed': 'Messages could not be loaded.',
  'lounge.messageList.retry': 'Try again',
  'lounge.notificationDialog.loadError':
    'Your notification settings for this server could not be loaded.',
  'lounge.notificationDialog.retry': 'Try again',
  'lounge.scrapbook.removalActionFailed': 'That did not work. Try again.',
  'lounge.scrapbook.shelfClearFailed': 'The shelf was not cleared. Try again.',
  'lounge.poll.votersLoading': 'Loading voters…',
  'lounge.poll.votersFailed': 'The voters could not be loaded.',
  'lounge.systemMessage.threadStarterFailed':
    'The message this thread started from could not be loaded.',
  'lounge.reactors.removeError': 'The reaction was not removed. Try again.',
  'lounge.fullProfile.loadFailed':
    'This could not be loaded. Close and open the profile to try again.',
  'moderation.reportDialog.blockFailed':
    'Your report was sent, but blocking this person did not work. Block them from their profile.',
  'admin.graph.lookupFailed': 'The search did not work. Try again.',
  'settings.extensions.retry': 'Try again',
  'settings.profile.retry': 'Try again',
  'settings.activity.loadError':
    'Your sharing choices could not be loaded, so they are locked for now.',
  'settings.activity.retry': 'Try again',
} satisfies Record<string, string>;
