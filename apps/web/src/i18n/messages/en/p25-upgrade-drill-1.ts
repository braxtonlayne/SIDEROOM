export default {
  'admin.system.storageTitle': 'Stored data',
  'admin.system.storageHint': 'Measured once a day by listing the bucket.',
  'admin.system.storageNone': 'Not measured yet. The first daily run fills this in.',
  'admin.system.storageBucketObjects': 'Object store (everything)',
  'admin.system.storageBucketLibrary': 'Library media',
  'admin.system.storageBucketCinema': 'Cinema media',
  'admin.system.storageFigure': '{bytes} in {objects} objects, measured {when}',
  'admin.system.storagePartial':
    'at least {bytes} in {objects} objects (the walk hit its limit), measured {when}',
} satisfies Record<string, string>;
