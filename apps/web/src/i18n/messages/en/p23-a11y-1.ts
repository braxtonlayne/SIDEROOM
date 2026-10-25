export default {
  // Live announcements for uploads (RDY-A11Y-003, keyboard flow 5): the progress bar alone is not announced.
  'a11y.announce.uploadStarted': 'Uploading {name}',
  'a11y.announce.uploadDone': 'Uploaded {name}',
  'a11y.announce.uploadFailed': 'Upload of {name} failed',
  // Landmarks for page content that sat outside any (axe `region`).
  'a11y.landmark.cinemaHeader': 'Cinema header',
  'a11y.landmark.libraryToolbar': 'Library toolbar',
  'a11y.landmark.titleStrip': 'Window bar',
  'a11y.landmark.homeSidebar': 'Direct messages',
} satisfies Record<string, string>;
