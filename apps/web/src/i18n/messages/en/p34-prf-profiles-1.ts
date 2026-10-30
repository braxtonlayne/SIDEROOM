// PRF-PRO-001 (D-382): the new profile blocks' editor strings and User Settings > Profile > Who can see my profile.
// (The strings the block renderer and the public page read are in p34-prf-profiles-core.)
export default {
  'profile.editor.kindLinks': 'Links',
  'profile.editor.kindServers': 'Featured servers',
  'profile.editor.kindPlaying': 'Now playing',
  'profile.editor.linksTitle': 'Title (optional)',
  'profile.editor.linksList': 'Links',
  'profile.editor.linksHint': 'One per line, as: label | https://address. Up to 12.',
  'profile.editor.serversLegend': 'Servers to show (up to 6)',
  'profile.editor.serversHint':
    'A server appears on your page only to people who are in it, or when it is listed in the directory.',
  'profile.editor.noServers': 'You are not in any servers yet.',
  'profile.editor.playingAuto': 'What I am playing now',
  'profile.editor.playingAutoHint':
    'Shows your current track or watch party to people who may see your presence. It is not shown to visitors without an account.',
  'profile.privacy.title': 'Who can see my profile',
  'profile.privacy.desc':
    'Choose who sees your profile page and each part of it. A new profile starts private.',
  'profile.privacy.loading': 'Loading your profile privacy',
  'profile.privacy.loadError': 'Your profile privacy could not be loaded.',
  'profile.privacy.saveError': 'Your change could not be saved. Try again.',
  'profile.privacy.errorTitle': 'Something went wrong',
  'profile.privacy.preset': 'Preset',
  'profile.privacy.presetPublic': 'Public',
  'profile.privacy.presetMembers': 'Members only',
  'profile.privacy.presetFriends': 'Friends only',
  'profile.privacy.presetPrivate': 'Private',
  'profile.privacy.presetPublicHelp':
    'Your page, links, about me and pictures are open to anyone on the web once this server allows public profiles. Top friends, now playing and the guestbook stay for signed-in members.',
  'profile.privacy.presetMembersHelp':
    'Everyone with an account on this server can see your page and all of it.',
  'profile.privacy.presetFriendsHelp': 'Only your friends can see your page.',
  'profile.privacy.presetPrivateHelp': 'Only you can see your page.',
  'profile.privacy.presetCustomHelp':
    'Custom: you changed individual audiences below. Pick a preset to start over.',
  'profile.privacy.walledTitle': 'Public pages are off on this server',
  'profile.privacy.walledBody':
    'The people who run this server have not turned on public profiles, so no one without an account can see any page. Anything you set to public is kept, but is shown to members only.',
  'profile.privacy.noIndex': 'Search engines never index SIDE ROOM profiles.',
  'profile.privacy.advanced': 'Advanced: an audience for each part',
  'profile.privacy.fieldPage': 'Your page',
  'profile.privacy.fieldPageHint':
    'Nobody below this audience sees any of it, whatever the parts say.',
  'profile.privacy.clampNote':
    'A part can be stricter than your page, never looser. Choices that would be looser are not offered.',
  'profile.privacy.fieldLinks': 'Links',
  'profile.privacy.fieldBio': 'About me',
  'profile.privacy.fieldMedia': 'Pictures and gallery',
  'profile.privacy.fieldTop8': 'Top friends and featured servers',
  'profile.privacy.fieldNowPlaying': 'Now playing and now watching',
  'profile.privacy.fieldGuestbook': 'Guestbook',
  'profile.privacy.fieldRing': 'Status rings',
  'profile.privacy.ringHint':
    'A ceiling on top of who each ring was shared with: a ring shared with friends never shows to anyone else.',
  'profile.privacy.tierPublic': 'Anyone on the web',
  'profile.privacy.tierMembers': 'Members of this server',
  'profile.privacy.tierMutualServers': 'People who share a server with me',
  'profile.privacy.tierFriends': 'Friends',
  'profile.privacy.tierOnlyMe': 'Only me',
  'profile.privacy.publicGuestbook': 'Let visitors without an account leave a note',
  'profile.privacy.publicGuestbookHint':
    'Needs a public page and a public guestbook. Notes are plain text without links and wait for your approval.',
  'profile.privacy.previewTitle': 'View as',
  'profile.privacy.viewAs': 'View my page as',
  'profile.privacy.viewPublic': 'Public web',
  'profile.privacy.viewMember': 'Member',
  'profile.privacy.viewShares': 'Shares a server',
  'profile.privacy.viewFriend': 'Friend',
  'profile.privacy.viewRegion': 'Your page as that viewer sees it',
  'profile.privacy.viewLoading': 'Loading the preview',
  'profile.privacy.viewNothing': 'This viewer sees nothing: your page is not open to them.',
  'profile.privacy.viewNoBlocks': 'This viewer sees your page, but none of its parts.',
} satisfies Record<string, string>;
