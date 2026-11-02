// XC-VIS-002 (polish round, media), second fragment: the player and Cinema pages.
export default {
  // The title page opened, but the video file behind it is missing or still being prepared.
  'cinema.watch.notReadyTitle': 'This video is not ready yet',
  'cinema.watch.notReadyBody': 'It may still be processing. Try again in a little while.',
  // Shelves: a kind or shelf filter (not a search) left nothing; the button resets search and filters.
  'shelves.noMatchFilters': 'Nothing matches these filters.',
  'shelves.clearFilters': 'Clear filters',
  // The storage browser (a page without app chrome): back to where it was opened, usually Settings > Storage.
  'storage.browser.back': 'Back',
  // The Study: a shelf folder could not be read.
  'study.retry': 'Try again',
  // Watch party: the party is running but its video could not be loaded for this viewer.
  'watchparty.overlays.mediaFailed': 'The video could not be loaded',
  'watchparty.overlays.mediaFailedBody':
    'The party is still on. Try leaving and joining again in a moment.',
} satisfies Record<string, string>;
