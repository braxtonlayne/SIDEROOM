export default {
  // Server Settings > Features (FTR-*)
  'settings.server.features': 'Features',
  'settings.features.title': 'Features',
  'settings.features.description':
    'Turn optional features on for this server and set them up. Everything is off until you switch it on.',
  'settings.features.loading': 'Loading features',
  'settings.features.none': 'No optional features are available.',
  'settings.features.platformOff':
    'The platform operator has switched this feature off for every server, so it does nothing here for now.',
  'settings.features.enable': 'Enable {name}',
  'settings.features.showSettings': 'Show settings for {name}',
  'settings.features.hideSettings': 'Hide settings for {name}',
  'settings.features.save': 'Save {name}',
  'settings.features.reset': 'Reset {name}',
  'settings.features.saved': 'Saved.',

  // Temporary roles panel
  'settings.features.temp.title': 'Temporary roles',
  'settings.features.temp.empty': 'Nobody has a temporary role right now.',
  'settings.features.temp.line': '{user} has {role} until {until}',
  'settings.features.temp.end': 'End now',
  'settings.features.temp.revoke': 'End {role} for {user}',
  'settings.features.temp.member': 'Member',
  'settings.features.temp.role': 'Role',
  'settings.features.temp.length': 'For how long',
  'settings.features.temp.give': 'Give role',
  'settings.features.temp.granted': 'Role given.',
  'settings.features.temp.hour': '1 hour',
  'settings.features.temp.day': '1 day',
  'settings.features.temp.week': '7 days',
  'settings.features.temp.month': '30 days',

  // Rows of a list in a schema form
  'admin.schemaFormRows.add': 'Add to {label}',
  'admin.schemaFormRows.remove': 'Remove {label} {n}',
  'admin.schemaFormRows.row': '{label} {n}',
  'admin.schemaFormRows.none': 'None',
} satisfies Record<string, string>;
