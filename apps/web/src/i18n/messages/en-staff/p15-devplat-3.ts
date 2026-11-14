export default {
  // Review Queues > Packages
  'admin.packages.title': 'Packages waiting for review',
  'admin.packages.empty': 'No packages are waiting for review.',
  'admin.packages.loadFailed': 'The package queue could not be loaded.',
  'admin.packages.by': 'by {author}, id {slug}',
  'admin.packages.warnings': 'Warnings',
  'admin.packages.files': '{count, plural, one {# file} other {# files}}',
  'admin.packages.filesLabel': 'Files in the package',
  'admin.packages.download': 'Download the zip of {name}',
  'admin.packages.approve': 'Approve',
  'admin.packages.reject': 'Reject',
  'admin.packages.approveAria': 'Approve {name} {version}',
  'admin.packages.rejectAria': 'Reject {name} {version}',
  'admin.packages.approveTitle': 'Approve this version',
  'admin.packages.rejectTitle': 'Reject this version',
  'admin.packages.confirm': 'Confirm',
  'admin.packages.noteLabel': 'Note for the creator',
  'admin.packages.decided': 'Decision recorded.',
  'admin.packages.summaryTheme':
    '{count, plural, one {# token} other {# tokens}} set, {scheme} scheme',
  'admin.packages.summaryLayout': '{count, plural, one {# block} other {# blocks}}',
  'admin.packages.summaryRecipe': 'Configures: {modules}',
  'admin.packages.summaryTemplate':
    '{channels, plural, one {# channel} other {# channels}}, {roles, plural, one {# role} other {# roles}}',
  'admin.packages.summaryPack': '{count, plural, one {# item} other {# items}}',
} satisfies Record<string, string>;
