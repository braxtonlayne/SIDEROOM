// UTL-DEV-001: the encrypted scratchpad.
export default {
  'tools.notes.privacy':
    'Your notes are encrypted in this browser before they are saved. The server only keeps scrambled text it cannot read, and neither can the people who run it.',
  'tools.notes.loading': 'Opening your notes…',
  'tools.notes.retry': 'Try again',
  'tools.notes.cancel': 'Cancel',
  'tools.notes.lostKey':
    'If you lose the key and your passphrase, your notes are gone for good. Nobody can recover them for you, including the people who run this server.',

  'tools.notes.setupTitle': 'Set up private notes',
  'tools.notes.passphrase': 'Passphrase',
  'tools.notes.passphraseAgain': 'Passphrase again',
  'tools.notes.passphraseShort': 'Use at least {n} characters.',
  'tools.notes.passphraseMismatch': 'The two passphrases are different.',
  'tools.notes.create': 'Create my key',
  'tools.notes.creating': 'Creating…',

  'tools.notes.restoreTitle': 'Open notes from another browser',
  'tools.notes.recoveryFile': 'Recovery file',
  'tools.notes.restore': 'Open my notes',
  'tools.notes.otherBrowser':
    '{n, plural, one {You have # note} other {You have # notes}} on this account, but this browser does not have the key. Open them with your recovery file and passphrase.',
  'tools.notes.startOverTitle': 'No recovery file?',
  'tools.notes.startOverBody':
    'Without the recovery file and passphrase the old notes cannot be opened. You can delete them and start again with a new key.',
  'tools.notes.startOver': 'Delete old notes and start again',
  'tools.notes.startOverConfirm': 'Yes, delete them for good',

  'tools.notes.newNote': 'New note',
  'tools.notes.saved': 'Saved',
  'tools.notes.saving': 'Saving…',
  'tools.notes.list': 'Your notes',
  'tools.notes.empty': 'No notes yet.',
  'tools.notes.untitled': 'Untitled',
  'tools.notes.unreadable': 'Unreadable note',
  'tools.notes.unreadableBody':
    'This note could not be opened with your key. It may be damaged, or made with another key.',
  'tools.notes.text': 'Note',
  'tools.notes.delete': 'Delete note',
  'tools.notes.downloadRecovery': 'Download recovery file',
  'tools.notes.downloadText': 'Download as text',
  'tools.notes.lock': 'Forget the key on this browser',
  'tools.notes.lockBody':
    'This removes the key from this browser. You will need your recovery file and passphrase to open the notes here again.',
  'tools.notes.lockConfirm': 'Forget the key',

  'tools.notes.problem.offline':
    'Could not reach the server. Your last changes may not be saved yet.',
  'tools.notes.problem.full': 'You have reached the limit of 500 notes.',
  'tools.notes.problem.tooBig': 'That note is too long. A note can hold about 64 KB of text.',
  'tools.notes.problem.badPassphrase': 'That passphrase does not open this recovery file.',
  'tools.notes.problem.badFile': 'That is not a recovery file this tool can read.',
  'tools.notes.problem.unsupported': 'This browser cannot do the encryption private notes need.',
} satisfies Record<string, string>;
