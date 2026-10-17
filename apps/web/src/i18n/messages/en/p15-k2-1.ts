// K-21 (D-263): the server requires encrypted calls and this browser cannot join one.
// K-16 and K-17 (D-259, D-260): profile page gallery and uploaded pictures.
// K-20 (D-262): move a Discord identity to another account.
export default {
  'voice.e2ee.required': 'This server requires encrypted calls; your browser cannot join.',
} satisfies Record<string, string>;
