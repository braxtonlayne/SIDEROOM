export default {
  // G-97 Enrolment prompt for 900123 (D-223)
  'admin.reauth.enrolTitle': 'This server requires two-factor authentication',
  'admin.reauth.enrolDescription': 'Moderator actions here need a second factor on your account.',
  'admin.reauth.enrolNotice':
    'Set up an authenticator app or a passkey in Settings, My Account, Two-factor authentication, then try again.',
  'admin.reauth.enrolOpen': 'Open My Account',
  // G-95 Protection settings on Message Requests and links (D-224)
  'lounge.requests.reasonNewAccount': 'This account is {days, plural, one {# day} other {# days}} old.',
  'lounge.requests.reasonServerFilter':
    'A server you share filters direct messages from people who are not your friends.',
  'lounge.links.suspiciousWarning':
    'The link text shows a different address. It opens {host}.',
} satisfies Record<string, string>;
