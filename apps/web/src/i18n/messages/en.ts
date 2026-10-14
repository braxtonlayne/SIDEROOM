/**
 * English source catalog (XC-I18N-001). Keys are `area.screen.thing`; values are ICU MessageFormat.
 *
 * Rules for contributors:
 * - Use the key in code as a literal: `t('entrance.login.title')`. `catalog.test.ts` scans the source for
 *   `t('...')` calls and fails on a key that is not defined here.
 * - Plurals and numbers go through ICU (`{n, plural, one {# minute} other {# minutes}}`), never string
 *   concatenation. A plain apostrophe is fine; quote a literal brace as `'{'`.
 * - Keep one sentence per key. Translators see only the string, so add a comment above when the context
 *   is not obvious.
 * - Other locales live in `./<code>.ts` (typed `Catalog`) and are registered in `../runtime.ts`; any missing
 *   key falls back to this file.
 */
export const en = {
  // Entrance chrome
  'entrance.shell.home': 'SIDE ROOM home',
  'entrance.shell.tagline': 'A private corner of the internet',
  'entrance.shell.memberAccess': 'Member access',
  'entrance.shell.backToEntrance': 'Back to the entrance',
  'entrance.shell.footerLine': 'Not everywhere.',
  'entrance.shell.footerAccent': 'Just here.',
  'entrance.shell.invitationOnly': 'By invitation only',
  'entrance.shell.houseNotes': 'House notes',
  'entrance.shell.copyright': 'SIDE ROOM © {year}',
  'entrance.shell.skipToContent': 'Skip to main content',

  // Landing
  'entrance.landing.eyebrow': 'Good things stay between us.',
  'entrance.landing.heading': 'The internet, with the door',
  'entrance.landing.headingAccent': 'closed.',
  'entrance.landing.lede1': 'A little less noise. A little more belonging.',
  'entrance.landing.lede2': 'A place for your people, and everything you’re into.',
  'entrance.landing.inviteLabel': 'Your invitation',
  'entrance.landing.invitePlaceholder': 'Enter your invite code',
  'entrance.landing.checking': 'Checking…',
  'entrance.landing.check': 'Check invitation',
  'entrance.landing.valid': "You're on the list.",
  'entrance.landing.invalid': "That invitation doesn't open this door.",
  'entrance.landing.closed': "The door's closed for now. We're at capacity.",
  'entrance.landing.limited': 'Too many tries. Take a breath and try again later.',
  'entrance.landing.error': 'Something went wrong on our side. Try again in a moment.',
  'entrance.landing.required': 'Invitation required. Always.',
  'entrance.landing.noInvitation': 'No invitation?',
  'entrance.landing.est': 'Est. after hours',
  'entrance.landing.invOnly': 'By invitation only',
  'entrance.landing.vertical': 'Some doors are worth finding.',
  'entrance.landing.footLine': 'The outside is quiet. The inside is yours.',
  'entrance.landing.glimpse': 'A glimpse inside',
  'entrance.landing.stripTitle': 'One door.',
  'entrance.landing.stripAccent': 'A few different worlds.',
  'entrance.landing.stripSub': 'For members, together.',
  'entrance.landing.world.lounge.title': 'The lounge',
  'entrance.landing.world.lounge.copy': 'Good company. Open conversations.',
  'entrance.landing.world.cinema.title': 'The cinema',
  'entrance.landing.world.cinema.copy': 'Something worth staying up for.',
  'entrance.landing.world.library.title': 'The library',
  'entrance.landing.world.library.copy': 'One more chapter. Then another.',

  // Shared auth layout
  'entrance.auth.welcome': 'Welcome',
  'entrance.auth.foot': 'A little less noise. A little more belonging.',

  // Login
  'entrance.login.eyebrow': 'Your usual place.',
  'entrance.login.heading': 'Good to have you',
  'entrance.login.headingAccent': 'back.',
  'entrance.login.copy1': 'Same people. New stories.',
  'entrance.login.copy2': 'Pick up where you left off.',
  'entrance.login.footLabel': 'Member entrance / 01',
  'entrance.login.formEyebrow': 'On the other side',
  'entrance.login.title': 'Come on in.',
  'entrance.login.sub': 'Sign in to your corner of the internet.',
  'entrance.login.discord': 'Continue with Discord',
  'entrance.login.divider': 'or make yourself at home',
  'entrance.login.id': 'Username or email',
  'entrance.login.idPlaceholder': 'Your username or email address',
  'entrance.login.idRequired': 'Enter your username or email.',
  'entrance.login.forgot': 'Forgot password?',
  'entrance.login.password': 'Password',
  'entrance.login.passwordPlaceholder': 'Your password',
  'entrance.login.passwordRequired': 'Enter your password.',
  'entrance.login.submit': 'Step inside',
  'entrance.login.submitting': 'Stepping inside…',
  'entrance.login.newHere': 'For people already on the list.',
  'entrance.login.newHereSub': 'New here? You’ll need an invitation.',
  'entrance.login.discordUnlinked':
    "No SIDE ROOM account is linked to that Discord account. You'll need an invitation.",
  'entrance.login.tooMany':
    'Too many attempts. Try again in {minutes, plural, one {# minute} other {# minutes}}.',
  'entrance.login.mismatch': "That doesn't match our list.",
  'entrance.login.unreachable': "We couldn't reach the door. Check your connection and try again.",

  // Register
  'entrance.register.eyebrow': 'Your invitation worked.',
  'entrance.register.heading': 'Make yourself',
  'entrance.register.headingAccent': 'at home.',
  'entrance.register.copy': 'A few details and the door is yours.',
  'entrance.register.footLabel': 'Member entrance / 00',
  'entrance.register.formEyebrow': 'Almost inside',
  'entrance.register.title': 'Open the door.',
  'entrance.register.sub': 'Create your account.',
  'entrance.register.discordLinked': 'Linked to Discord as @{username}',
  'entrance.register.discordUnlink': 'Unlink Discord',
  'entrance.register.divider': 'or start fresh',
  'entrance.register.email': 'Email',
  'entrance.register.username': 'Username',
  'entrance.register.displayName': 'Display name (optional)',
  'entrance.register.password': 'Password',
  'entrance.register.passwordHint': 'At least 10 characters.',
  'entrance.register.confirm': 'Confirm password',
  'entrance.register.checkingName': 'Checking…',
  'entrance.register.nameFree': 'Available.',
  'entrance.register.mismatch': "Those passwords don't match.",
  'entrance.register.rulesRequired': 'Please confirm you have read the house notes.',
  'entrance.register.nameTaken': 'That username is taken.',
  'entrance.register.rulesBefore': 'I’ve read the',
  'entrance.register.rulesLink': 'house notes',
  'entrance.register.submit': 'Open the door',
  'entrance.register.submitting': 'Opening…',
  'entrance.register.footnote': 'Your invitation is single-use. Once you’re in, it’s spent.',
  'entrance.register.tooMany': 'Too many tries. Take a breath and try again later.',
  'entrance.register.badInvite': "That invitation doesn't open this door.",
  'entrance.register.failed': "We couldn't open the door. Check the details and try again.",
  'entrance.register.unreachable':
    "We couldn't reach the door. Check your connection and try again.",

  // Verify-email banner
  'entrance.verify.idle': 'Verify your email to keep your account safe.',
  'entrance.verify.sending': 'Sending a fresh link…',
  'entrance.verify.sent': 'Check your inbox for the link.',
  'entrance.verify.wait': 'A link was sent a moment ago. Try again in a minute.',
  'entrance.verify.error': "We couldn't send the link. Try again in a moment.",
  'entrance.verify.resend': 'Resend link',

  // Passkey button
  'entrance.passkey.use': 'Use a passkey',
  'entrance.passkey.tooMany': 'Too many attempts. Wait a moment and try again.',
  'entrance.passkey.cancelled': 'Passkey sign-in was cancelled.',
  'entrance.passkey.failed': "That passkey didn't work. Try another way in.",

  // Static pages
  'entrance.static.noInvitation.title': 'No invitation?',
  'entrance.static.noInvitation.body':
    'SIDE ROOM is invitation-only. Ask someone you know who’s a member.',

  // House notes (the page the Library footer and the registration form link to)
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

  // Support (the Help link in the Library footer)
  'entrance.support.title': 'Help',
  'entrance.support.intro':
    'The common ways to get unstuck. If none of these fit, ask the person who invited you.',
  'entrance.support.signin.title': 'I cannot sign in',
  'entrance.support.signin.body': 'Reset your password with the link on the sign-in page.',
  'entrance.support.signin.link': 'Reset my password',
  'entrance.support.twofa.title': 'I lost my second factor',
  'entrance.support.twofa.body':
    'Use one of your recovery codes on the second step of signing in. Without one, a staff member can reset it for you after confirming who you are.',
  'entrance.support.invite.title': 'I do not have an invitation',
  'entrance.support.invite.body':
    'SIDE ROOM is invitation-only. Ask someone you know who is a member.',
  'entrance.support.invite.link': 'More about invitations',
  'entrance.support.rules.title': 'Something is wrong here',
  'entrance.support.rules.body':
    'Report it to a server moderator, or read the house notes for what is and is not allowed.',
  'entrance.support.rules.link': 'House notes',

  // Settings shell (adopted by the user and server settings screens)
  'settings.close': 'Close settings',
  'settings.saved': 'Settings saved',
  'settings.group.user': 'User Settings',
  'settings.group.app': 'App Settings',
  'settings.user.account': 'My Account',
  'settings.user.profile': 'Profile',
  'settings.user.privacy': 'Privacy & Safety',
  'settings.user.appearance': 'Appearance',
  'settings.user.notifications': 'Notifications',
  'settings.user.keybinds': 'Keybinds',
  'settings.user.language': 'Language',
  'settings.user.advanced': 'Advanced',
  'settings.server.overview': 'Overview',
  'settings.server.roles': 'Roles',
  'settings.server.channels': 'Channels',
  'settings.server.emoji': 'Emoji & Stickers',
  'settings.server.members': 'Members',
  'settings.server.invites': 'Invites',
  'settings.server.bans': 'Bans',
  'settings.server.integrations': 'Integrations',
  'settings.server.automod': 'AutoMod',
  'settings.server.audit': 'Audit Log',
  'settings.server.community': 'Community',
  'settings.server.delete': 'Delete Server',
  'settings.language.title': 'Language',
  'settings.language.description': 'Choose the language SIDE ROOM is shown in.',
  'settings.language.clock': 'Time format',
  'settings.language.clockAuto': 'Follow the language',
  'settings.language.clock12': '12-hour',
  'settings.language.clock24': '24-hour',

  // Progressive web app
  'pwa.update.title': 'A new version is ready',
  'pwa.update.body': 'Reload to get the latest SIDE ROOM.',
  'pwa.update.action': 'Reload',
  'pwa.update.dismiss': 'Later',
  'pwa.install.title': 'Install SIDE ROOM',
  'pwa.install.body': 'Add it to your device for a faster, app-like window.',
  'pwa.install.action': 'Install',
  'pwa.install.dismiss': 'Not now',
  'pwa.install.ios.title': 'Add SIDE ROOM to your Home Screen',
  'pwa.install.ios.step1': 'Tap the Share button in Safari.',
  'pwa.install.ios.step2': 'Choose “Add to Home Screen”.',
  'pwa.install.ios.step3': 'Tap Add.',
  'pwa.offline.title': "You're offline",
  'pwa.offline.body':
    "SIDE ROOM needs a connection. We'll pick up where you left off when you're back.",
  'pwa.offline.retry': 'Try again',

  // Accessibility
  'a11y.skipToContent': 'Skip to main content',
  'a11y.skipToNav': 'Skip to navigation',
  'a11y.mainLandmark': 'Main content',
  'a11y.announce.newMessages': '{count, plural, one {# new message} other {# new messages}}',
} as const;

export type MessageKey = keyof typeof en;
