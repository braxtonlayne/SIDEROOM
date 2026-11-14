// K-21 (D-263): the server requires encrypted calls and this browser cannot join one.
// K-16, K-17, K-18 (D-259, D-260): profile page pictures, uploads and the guestbook count.
// K-20 (D-262): move a Discord identity to another account.
export default {
  'admin.person.identitiesTitle': 'Discord identities',
  'admin.person.identitiesIntro':
    'The Discord accounts linked to the accounts of this person. A moved identity stays here as history.',
  'admin.person.identityLine': 'Discord {id} on @{holder}',
  'admin.person.identityLive': 'Linked {when}',
  'admin.person.identityMoved': 'Moved to @{to} on {when}',
  'admin.person.moveIdentity': 'Move to account',
  'admin.person.moveIdentityOf': 'Move the Discord identity {id} to another account',
  'admin.person.moveDescription':
    'The Discord identity moves to the account you pick, which joins this person if it is elsewhere. The old account is told by a system message and, when its address is verified, by email. The reason is kept in the access log.',
  'admin.person.moveConfirm': 'Move identity',
  'admin.person.moveTo': 'Move to account',
  'admin.person.moveOther': 'Another account (enter its id)',
  'admin.person.moveOtherHint': 'It is linked into this person by the move.',
  'admin.person.identityMovedDone': 'The identity was moved. The old account was messaged.',
  'admin.person.identityMovedMailed':
    'The identity was moved. The old account was messaged and emailed.',
} satisfies Record<string, string>;
