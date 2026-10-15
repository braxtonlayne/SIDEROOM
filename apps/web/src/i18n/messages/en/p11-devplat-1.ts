export default {
  // Slash commands from apps: option form
  'lounge.apps.form.title': 'Use /{name}',
  'lounge.apps.form.description': '{app}: {description}',
  'lounge.apps.form.run': 'Run command',
  'lounge.apps.form.cancel': 'Cancel',
  'lounge.apps.form.subcommand': 'Subcommand',
  'lounge.apps.form.pick': 'Pick one',
  'lounge.apps.form.pickSubcommand': 'Pick a subcommand to continue.',
  'lounge.apps.form.attachmentUnsupported':
    'The file option "{name}" is not supported yet, so this command cannot send files from here.',
  'lounge.apps.form.errRequired': 'This field is required.',
  'lounge.apps.form.errNumber': 'Enter a number.',
  'lounge.apps.form.errInteger': 'Enter a whole number.',
  'lounge.apps.form.errMin': 'Must be {min} or more.',
  'lounge.apps.form.errMax': 'Must be {max} or less.',
  'lounge.apps.form.errMinLength': 'Use at least {min} characters.',
  'lounge.apps.form.errMaxLength': 'Use at most {max} characters.',
  'lounge.apps.form.errChoice': 'Pick one of the listed choices.',
  'lounge.apps.rejected': 'Could not use that app: {message}',
  'lounge.apps.picker.hint': '{app}: {description}',
  'lounge.apps.picker.hintNoDescription': '{app}',

  // Modal an app opened
  'lounge.apps.modal.submit': 'Submit',
  'lounge.apps.modal.cancel': 'Cancel',

  // Ephemeral replies
  'lounge.apps.ephemeral.note': 'Only you can see this',
  'lounge.apps.ephemeral.dismiss': 'Dismiss message',

  // Apps menu on messages and people
  'lounge.apps.menu.apps': 'Apps',
  'lounge.apps.menu.empty': 'No apps available here',
} satisfies Record<string, string>;
