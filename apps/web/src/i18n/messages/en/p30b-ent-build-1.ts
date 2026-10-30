// Strings only the signed-in app and the build-time legal pages use (D-385: the public surfaces' core fragment
// 'entrance' carries only what a visitor's page needs). Moved here from entrance.ts by wave 30b.
export default {
  'entrance.houseNotes.title': 'House notes',
  'entrance.houseNotes.intro':
    'SIDE ROOM is a small, private place for people who know each other. Be kind, keep what’s said here here, and look after the room.',
  'entrance.houseNotes.rules.title': 'The house rules',
  'entrance.houseNotes.rules.1': 'Be kind. Disagree without being cruel.',
  'entrance.houseNotes.rules.2':
    'What is said here stays here. Do not share other members’ messages or details outside.',
  'entrance.houseNotes.rules.3':
    'Your invitation is your word. Invite people you would be happy to vouch for.',
  'entrance.houseNotes.rules.4':
    'Respect age ratings and content settings, and keep adult material where it belongs.',
  'entrance.houseNotes.rules.5':
    'Do not post anything illegal, and do not harass, spam or impersonate anyone.',
  'entrance.houseNotes.privacy.title': 'Privacy',
  'entrance.houseNotes.privacy.body1':
    'Messages are kept until they are deleted. A deleted message disappears from view at once and is purged after 30 days; until then only senior staff can see it.',
  'entrance.houseNotes.privacy.body2':
    'Voice and video are encrypted end to end and are never recorded. Passwords are stored hashed, and nothing is sent to advertisers or analytics services.',
  'entrance.houseNotes.privacy.body3':
    'You can export your data from Settings, and you decide whether your watching and reading activity is shared.',
  'entrance.houseNotes.terms.title': 'Terms',
  'entrance.houseNotes.terms.body':
    'This place is run by its operator for their community, and access is by invitation. The operator may remove content or accounts that break the house rules. Your operator can replace this page with their own terms.',
  'entrance.houseNotes.credits.title': 'Image credits',
  'entrance.houseNotes.credits.door':
    'The door photograph on the entrance pages was supplied by the owner.',
  'entrance.houseNotes.credits.fonts':
    'Typefaces: Newsreader, Hanken Grotesk, Inter and IBM Plex Mono, used under the SIL Open Font License.',
  'entrance.houseNotes.credits.licences': 'Font licences',
  'entrance.support.title': 'Help',
  'entrance.support.intro':
    'The common ways to get unstuck. If none of these fit, ask the person who invited you.',
  'entrance.support.notices.title': 'Third-party notices',
  'entrance.support.notices.body':
    'SIDE ROOM includes software under other licences that ask for attribution.',
  'entrance.support.notices.link': 'Read the third-party notices',
  'entrance.serverInvite.counts': '{online} online · {members} members',
  'entrance.serverInvite.invalid': 'This invite is invalid or has expired.',
  'entrance.serverInvite.accept': 'Accept invite',
  'entrance.template.tooFast': 'You are doing that too quickly. Wait a moment and try again.',
  'entrance.template.badName': 'Server names are 2 to 100 characters.',
  'entrance.template.failed': 'Could not create the server. Try again.',
  'entrance.template.invalid': 'This template does not exist or was deleted.',
  'entrance.template.start': 'Start a server from the template',
  'entrance.template.stats':
    '{channels} channels · {roles} roles · used {uses, plural, one {# time} other {# times}}',
  'entrance.template.by': ' · by {name}',
  'entrance.template.channelsLabel': 'Channels in this template',
  'entrance.template.voicePrefix': 'Voice: ',
  'entrance.template.name': 'Server name',
  'entrance.template.create': 'Create server',
  'entrance.invite.groupInvited': "You've been invited to join a group chat",
  'entrance.invite.groupName': 'a group chat',
  'entrance.invite.groupMembers': '{count} members',
  'entrance.houseNotes.privacy.operatorCountry': 'This server is run from {country}.',
  'entrance.houseNotes.privacy.operatorContact': 'Contact for questions about your data:',
} satisfies Record<string, string>;
