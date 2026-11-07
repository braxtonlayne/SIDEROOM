// SEC-AUD-008 / 014 (wave 53): emulator save-read failure and the failed-load states that keep Save off.
export default {
  'games.saves.readFailed':
    'Your save could not be read, so saving is off for this session. Your existing save is untouched. Reload to try again.',
  'lounge.guildProfile.loadError':
    'Your server profile could not be loaded, so saving is off. Nothing was changed.',
  'lounge.guildProfile.retry': 'Try again',
  'lounge.userDialogs.noteLoadError':
    'Your note could not be loaded, so saving is off. Nothing was changed.',
  'lounge.userDialogs.retry': 'Try again',
  'lounge.profilePopout.noteLoadError': 'Your note could not be loaded, so it is locked for now.',
  'lounge.profilePopout.noteSaveError': 'Your note was not saved.',
  'lounge.profilePopout.noteRetry': 'Try again',
  'settings.birthday.retry': 'Try again',
  'tools.notes.notSaved': 'Not saved. Your changes are still here.',
} satisfies Record<string, string>;
