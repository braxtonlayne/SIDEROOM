// RDY-ONB-001..003 (wave 23): the platform first run, the member handbook at /help and "What's new".
// The handbook agrees with docs/09-retention-and-privacy.md; change them together.
export default {
  // ---- First run (lounge/shell/Welcome.tsx) ----
  'lounge.firstRun.progress': 'Step {n} of {total}',
  'lounge.firstRun.next': 'Next',
  'lounge.firstRun.back': 'Back',
  'lounge.firstRun.skip': 'Skip for now',
  'lounge.firstRun.close': 'Close the welcome',
  'lounge.firstRun.finish': 'Start exploring',
  'lounge.firstRun.reopenHint': 'You can bring this back any time from Help.',
  'lounge.firstRun.intro.title': 'Three rooms, one door',
  'lounge.firstRun.intro.body':
    'You are in. SIDE ROOM has three sections, and the left rail takes you between them.',
  'lounge.firstRun.intro.lounge':
    'The Lounge: chat, voice and video with your servers and friends.',
  'lounge.firstRun.intro.library': 'The Library: manga, comics and books to read in the browser.',
  'lounge.firstRun.intro.cinema':
    'The Cinema: films, shows, anime and Live TV, on your own or in a watch party.',
  'lounge.firstRun.look.title': 'Make it yours',
  'lounge.firstRun.security.title': 'Protect your account',
  'lounge.firstRun.security.body':
    'Two-step sign-in keeps your account safe even if your password leaks. It takes a minute and you can do it right here.',
  'lounge.firstRun.security.start': 'Set it up here',
  'lounge.firstRun.security.done': 'Two-step sign-in is on. Thank you.',
  'lounge.firstRun.install.title': 'Install the app',
  'lounge.firstRun.install.body':
    'SIDE ROOM installs like an app, with its own window and an icon on your device.',
  'lounge.firstRun.install.action': 'Install SIDE ROOM',
  'lounge.firstRun.notify.title': 'Notifications',
  'lounge.firstRun.notify.body':
    'Nothing will ask for permission now. When you want alerts, open Settings, Notifications, or press a bell, and your browser will ask then.',
  'lounge.firstRun.notify.open': 'Open notification settings',
  'lounge.firstRun.server.title': 'About {name}',
  'lounge.firstRun.server.body':
    'This server has a guide for newcomers. Have a look when you are ready.',
  'lounge.firstRun.server.guide': 'Open the server guide',
  'lounge.firstRun.server.questions': 'Choose channels and roles',
  'lounge.helpMenu.whatsNew': "What's new",
  'lounge.helpMenu.whatsNewBadge': 'Help, there is news',
  'lounge.helpMenu.welcome': 'Show the welcome again',

  // ---- Handbook (routes/entrance/Handbook.tsx) ----
  'help.handbook.title': 'Handbook',
  'help.handbook.intro':
    'A short guide to living here: how to get in, how to stay safe, and where everything is. Each section names the screen it is about.',
  'help.handbook.contents': 'In this handbook',
  'help.handbook.whatsNew.link': "What's new in SIDE ROOM",
  'help.handbook.welcome.title': 'The welcome',
  'help.handbook.welcome.body':
    'When you first arrive there is a short welcome. You can skip it, and you can bring it back here at any time.',
  'help.handbook.welcome.button': 'Show the welcome again',
  'help.handbook.welcome.failed': 'The welcome could not be reopened. Try again in a moment.',
  'help.handbook.joining.title': 'Joining, invitations and credits',
  'help.handbook.joining.body1':
    'SIDE ROOM is invitation-only. You came in with a code from a member, and you can pass the favour on: every account starts with a handful of invite credits (five by default), and one credit lets one more person in.',
  'help.handbook.joining.body1Count':
    'SIDE ROOM is invitation-only. You came in with a code from a member, and you can pass the favour on: every account starts with {count, plural, one {# invite credit} other {# invite credits}}, and one credit lets one more person in.',
  'help.handbook.joining.body2':
    'Your codes, the credits you have left and, where your operator allows it, how to buy more are in Settings, Invites. A code is your word, so invite people you would be happy to answer for.',
  'help.handbook.joining.link': 'Open Invites',
  'help.handbook.signin.title': 'Signing in, two-step sign-in and passkeys',
  'help.handbook.signin.body1':
    'Sign in with your username or email and your password. If you turned on two-step sign-in, a second step follows: a code from your authenticator app, a recovery code or a passkey.',
  'help.handbook.signin.body2':
    'A passkey lets you sign in with your fingerprint, face or PIN and nothing to type. Add one, and set up the authenticator app, in Settings, My Account. Keep your recovery codes somewhere safe: without them only a staff member can reset the second step, after confirming who you are.',
  'help.handbook.signin.link': 'Open Account settings',
  'help.handbook.install.title': 'Installing SIDE ROOM on your phone',
  'help.handbook.install.ios':
    'On an iPhone or iPad, open SIDE ROOM in Safari, tap Share, then Add to Home Screen.',
  'help.handbook.install.android':
    'On Android, open it in Chrome, open the menu, then choose Install app. The Lounge also has an install button on the left rail while the browser offers one.',
  'help.handbook.install.notify':
    'Notifications are your choice. The browser asks only when you open Settings, Notifications, or press a bell. On an iPhone they work once the app is on the Home Screen.',
  'help.handbook.install.link': 'Open notification settings',
  'help.handbook.lounge.title': 'The Lounge, for people who know Discord',
  'help.handbook.lounge.intro':
    'The Lounge works the way you expect: servers, channels, threads, direct messages, roles, reactions, slash commands and bots. What a developer calls a guild is a server here. The differences worth knowing:',
  'help.handbook.lounge.diff1':
    'Voice and video are encrypted end to end and are never recorded. The server only relays what it cannot read.',
  'help.handbook.lounge.diff2':
    'A deleted message disappears from view at once and is purged after 30 days. Files over 30 MB expire seven days after upload.',
  'help.handbook.lounge.diff3':
    'There are no discriminators. Your username is unique, and your display name is separate.',
  'help.handbook.lounge.diff4':
    'Your watching and reading activity is shared with friends and people who share a server with you, never with strangers. You can turn it off in Settings, Activity Privacy.',
  'help.handbook.lounge.diff5':
    'Direct messages from people you do not know can wait in Message Requests until you accept them.',
  'help.handbook.lounge.link': 'Open the Lounge',
  'help.handbook.submit.title': 'Submitting to the Library and the Cinema',
  'help.handbook.submit.body':
    'Anyone can propose a series for the Library or a film or show for the Cinema. Staff review it before it goes live. Files that do not match the naming guide still upload; you fill in the details by hand.',
  'help.handbook.submit.libraryName':
    'Library: one folder per series, one folder per chapter, pages called page_001.jpg, page_002.png and so on. Or a .cbz archive called Series v01 c001.',
  'help.handbook.submit.movieName': 'Cinema films: Title (Year)/Title (Year).mkv',
  'help.handbook.submit.showName':
    'Cinema shows: Show (Year)/Season 01/Show - s01e01 - Episode Title.mkv',
  'help.handbook.submit.tags':
    'Text in brackets or parentheses is read as a tag and ignored for numbering, and spaces, dots, dashes and underscores are interchangeable.',
  'help.handbook.submit.libraryLink': 'Open Library submissions',
  'help.handbook.submit.cinemaLink': 'Open Cinema submissions',
  'help.handbook.parties.title': 'Watch parties',
  'help.handbook.parties.body':
    'Start a watch party from the page of a film or episode in the Cinema, invite friends, and everyone watches in step with chat beside the picture. If you connect a call, round camera bubbles appear over the stage.',
  'help.handbook.parties.link': 'Browse the Cinema',
  'help.handbook.tv.title': 'Live TV',
  'help.handbook.tv.body':
    'Live TV is a guide of channels that air programmes on a schedule, at the same moment for everyone. Open the guide, pick a channel and tune in. Between programmes you may see a slate while the next one starts.',
  'help.handbook.tv.link': 'Open the TV guide',
  'help.handbook.privacy.title': 'Privacy: what is stored, your copy, deleting and reporting',
  'help.handbook.privacy.stored':
    'Messages are kept until they are deleted. A deleted message disappears from view at once and is purged after 30 days; until then only senior staff can see it. Voice and video are never recorded. Sign-in addresses are kept for 180 days for security.',
  'help.handbook.privacy.export':
    'You can ask for a copy of your data in Settings, Privacy & Safety. It is built for you and offered as a download link for seven days. The same page can delete all of your messages and uploads, and Activity Privacy decides who sees what you watch and read.',
  'help.handbook.privacy.delete':
    'Deleting your account is permanent, and you are told what stays before you confirm: the invite graph, invoices, staff-side moderation history, and any messages you did not delete, shown as Deleted User.',
  'help.handbook.privacy.report':
    'Where your operator has switched reporting on, use Report in the menu of a message, a person, a series or a title. A moderator reads it.',
  'help.handbook.privacy.exportLink': 'Open Privacy settings',
  'help.handbook.privacy.deleteLink': 'Open Account settings to delete your account',
  'help.handbook.privacy.notesLink': 'Read the privacy summary in the house notes',
  'help.handbook.house.title': 'The house notes',
  'help.handbook.house.body':
    'The house rules, the plain-language privacy summary, the terms and the image credits live on one page. Your operator may have replaced them with their own.',
  'help.handbook.house.link': 'Read the house notes',

  // ---- What's new (routes/entrance/WhatsNew.tsx) ----
  'help.whatsNew.title': "What's new",
  'help.whatsNew.intro': 'What changed in SIDE ROOM, newest first.',
  'help.whatsNew.back': 'Back to the handbook',
  'help.whatsNew.empty': 'There is nothing to show yet.',
} satisfies Record<string, string>;
