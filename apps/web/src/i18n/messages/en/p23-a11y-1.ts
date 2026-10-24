export default {
  // Live announcements for uploads (RDY-A11Y-003, keyboard flow 5): the progress bar alone is not announced.
  'a11y.announce.uploadStarted': 'Uploading {name}',
  'a11y.announce.uploadDone': 'Uploaded {name}',
  'a11y.announce.uploadFailed': 'Upload of {name} failed',
} satisfies Record<string, string>;
