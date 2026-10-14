export default {
  // Web Push (XC-PWA-004)
  'pwa.push.title': 'Push notifications on this device',
  'pwa.push.label': 'Send push notifications to this device',
  'pwa.push.description':
    'Get mentions and direct messages even when SIDE ROOM is closed. This only affects this device.',
  'pwa.push.enabled': 'Push notifications are on for this device.',
  'pwa.push.disabled': 'Push notifications are off for this device.',
  'pwa.push.noBrowser': 'This browser does not support push notifications.',
  'pwa.push.noServer': 'This server has not turned on push notifications yet.',
  'pwa.push.blockedTitle': 'Notifications are blocked',
  'pwa.push.blockedBody':
    'Allow notifications for SIDE ROOM in your browser or device settings, then come back here.',
  'pwa.push.errorTitle': 'Could not change push notifications',
  'pwa.push.errorBody': 'Check your connection and try again.',
  // Share Target (XC-PWA-006)
  'pwa.share.title': 'Share to SIDE ROOM',
  'pwa.share.preview': 'Shared content',
  'pwa.share.empty':
    'Nothing was shared. Share a link or some text from another app to see it here.',
  'pwa.share.loading': 'Loading your channels…',
  'pwa.share.error': 'Could not load your channels. Try again in a moment.',
  'pwa.share.noTargets': 'You have no channels to share to yet.',
  'pwa.share.dms': 'Direct messages',
  // Live announcements (XC-A11Y-002)
  'a11y.announce.messageFrom': '{author}: {text}',
  'a11y.announce.messageFromAttachment': '{author} sent an attachment',
  'a11y.announce.voiceJoined': 'Joined voice channel {channel}',
  'a11y.announce.voiceLeft': 'Left the voice channel',
  'a11y.announce.voiceReconnecting': 'Voice connection lost. Reconnecting…',
  'a11y.announce.voiceReconnected': 'Voice connection restored',
  'a11y.announce.voiceFailed': 'The voice call could not connect',
  'a11y.announce.voiceDropped': 'The voice call was disconnected',
  'a11y.announce.micMuted': 'Microphone muted',
  'a11y.announce.micUnmuted': 'Microphone unmuted',
  'a11y.announce.deafened': 'Deafened',
  'a11y.announce.undeafened': 'Undeafened',
} satisfies Record<string, string>;
