// SEC-AUD-041 (wave 55, D-573): a change to how the account signs in takes the password, then the second factor.
export default {
  'settings.reauth.description':
    'Changing how you sign in needs your second factor too. Once confirmed, it stays open for {minutes} minutes on this session.',
  'settings.discord.linkTitle': 'Link Discord',
  'settings.discord.linkDesc':
    'Enter your password to add Discord as a way to sign in. An open session alone cannot add one.',
  'settings.discord.linkContinue': 'Continue',
} satisfies Record<string, string>;
