export default {
  // ---- wave 37 integration: the way into the emulator from the signed-in app ----
  'lounge.chrome.games': 'Games',
  // The emulator page policy (cspProbe.ts): games need a page loaded at /games.
  'games.policy.reloadTitle': 'Games need a fresh page',
  'games.policy.reload':
    'This page was opened from another part of the app, so it cannot start games yet. Loading it again leaves the voice call; rejoin it afterwards from the lounge and come back here.',
  'games.policy.reloadButton': 'Load Games again',
  'games.policy.blockedTitle': 'Games cannot start on this page',
  'games.policy.blocked':
    'The page security policy does not allow the emulator. The host may be running an older server version; ask its operator.',
} satisfies Record<string, string>;
