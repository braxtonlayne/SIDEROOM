/** Release notes shown by Settings > What's New. Newest first; plain text only. */
export interface ReleaseNote {
  title: string;
  items: string[];
}

export const WHATS_NEW: ReleaseNote[] = [
  {
    title: 'Settings, filled in',
    items: [
      'Edit your display name, username, avatar and banner, and set a nickname per server.',
      'Appearance, accessibility, text and images, notifications and keybinds are saved to your account and follow you between devices.',
      'Choose who can send you friend requests and direct messages.',
      'Search settings from the box above the list.',
    ],
  },
];
