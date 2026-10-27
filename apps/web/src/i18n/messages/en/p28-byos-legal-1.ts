export default {
  // ---- Admin > Storage (BYOS-LEG-001, phase 28) ----
  'admin.adminApp.section.storage': 'Storage',
  'admin.storage.title': 'Storage',
  'admin.storage.description':
    "Every connected bucket: whose it is and whether it is shown. Delisting hides a bucket from everyone and tells its owner why. It never touches the bucket: it is the member's property. To delist one object, add it to a takedown notice (Moderation, Takedowns).",
  'admin.storage.filters': 'Filters',
  'admin.storage.statusFilter': 'Status',
  'admin.storage.statusAny': 'Any status',
  'admin.storage.status.active': 'Active',
  'admin.storage.status.failing': 'Failing',
  'admin.storage.status.disabled': 'Switched off',
  'admin.storage.status.delisted': 'Delisted',
  'admin.storage.ownerKind.user': 'Member',
  'admin.storage.ownerKind.guild': 'Server',
  'admin.storage.ownerKind.platform': 'Instance',
  'admin.storage.listLabel': 'Storage buckets',
  'admin.storage.emptyTitle': 'No buckets',
  'admin.storage.emptyDescription': 'No bucket matches this filter.',
  'admin.storage.meta':
    '{owner} · {provider} · bucket {bucket} · key ending {hint} · connected {when}',
  'admin.storage.delistedSince': 'Delisted {when}: {reason}',
  'admin.storage.delistedObjects': '{count} delisted objects',
  'admin.storage.delistButton': 'Delist',
  'admin.storage.delistAria': 'Delist {label}',
  'admin.storage.delistTitle': 'Delist this bucket?',
  'admin.storage.delistDescription':
    'Nothing in this bucket will be shown to anyone, the owner included, until you relist it. The owner is told why. The bucket and its objects are not touched.',
  'admin.storage.delistedNotice': 'Delisted {label}. The owner has been told.',
  'admin.storage.relistButton': 'Relist',
  'admin.storage.relistAria': 'Relist {label}',
  'admin.storage.relistTitle': 'Relist this bucket?',
  'admin.storage.relistDescription':
    'The bucket returns to the status it had before it was delisted.',
  'admin.storage.relistedNotice': 'Relisted {label}.',
  // ---- Takedowns: an object in a member's bucket ----
  'admin.takedowns.item.storageObject': 'Object in a member bucket',
  'admin.takedowns.addMountId': 'Bucket id',
  'admin.takedowns.addObjectKey': 'Object key in the bucket',
  'admin.takedowns.objectKey': 'key {key}',
} as const;
