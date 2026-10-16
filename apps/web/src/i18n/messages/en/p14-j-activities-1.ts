export default {
  // Portal Activities tab: bundles, development URL, review (J-12)
  'developers.activities.intro':
    'An Activity is a web app that runs inside a voice or text channel. You upload it as a zip of static files; the server hosts it on its own address, in a sandbox, and staff review the exact files before it can be started.',
  'developers.activities.redirectAuto':
    'Your Activity address is a redirect URI of this application automatically, so you do not register it on the OAuth2 tab.',
  'developers.activities.bundleHeading': 'Bundle',
  'developers.activities.bundleIntro':
    'A zip of at most 32 MiB unpacked and 2,000 files, with index.html at the top. Allowed files: html, js, mjs, css, json, map, wasm, png, jpg, jpeg, webp, gif, svg, ico, woff2, mp3, ogg, webm, txt.',
  'developers.activities.drop': 'Drop a zip here',
  'developers.activities.choose': 'Choose a zip',
  'developers.activities.pickZip': 'Activity bundle (zip)',
  'developers.activities.uploading': 'Uploading the bundle',
  'developers.activities.uploadFailed': 'The bundle could not be uploaded.',
  'developers.activities.tooLarge': 'That file is larger than 32 MiB.',
  'developers.activities.uploaded':
    'Uploaded: {n} files, {size} unpacked. Submit it to put it in review.',
  'developers.activities.bundlesLabel': 'Bundles',
  'developers.activities.bundleMeta': '{n} files, {size}',
  'developers.activities.bundleDraft': 'Uploaded',
  'developers.activities.bundlePending': 'In review',
  'developers.activities.bundleApproved': 'Approved',
  'developers.activities.bundleRejected': 'Not approved',
  'developers.activities.bundleSuperseded': 'Replaced',
  'developers.activities.showFiles': 'Show files',
  'developers.activities.hideFiles': 'Hide files',
  'developers.activities.filesWhat': 'file list',
  'developers.activities.filesLabel': 'Files in the bundle',
  'developers.activities.stateDraft': 'Nothing submitted yet. Upload a bundle and submit it.',
  'developers.activities.approvedHash': 'Approved bundle (SHA-256):',
  'developers.activities.submitHint':
    'Staff approve one bundle by its fingerprint, and only that bundle is served while your application is released. A new upload is a new draft; changing the URL mappings withdraws the approval.',
  'developers.activities.devUrl': 'Development URL',
  'developers.activities.devUrlHint':
    'While your application is in development, your Activity loads live from this https address for you and your testers. It is never reviewed and is ignored once the application is released.',
  'developers.activities.mappingsIntro':
    'Your Activity can only talk to its own address. A mapping sends requests under a path, like /api, to a server of yours, with GET, POST, PUT, PATCH and DELETE (bodies up to 1 MiB). Files of the bundle are read-only.',

  // Launcher, panel and docked window (J-15)
  'activities.launcher.description': 'Pick an Activity to start for everyone in this channel.',
  'activities.launcher.development': 'Development',
  'activities.panel.development': 'Development',
  'activities.window.label': '{name} Activity window',
  'activities.window.return': 'Return',
  'activities.window.leave': 'Leave',

  // OAuth2 tab (J-13)
  'developers.oauth2.implicitLabel': 'Automatic redirect URI of the Activity',
  'developers.oauth2.implicitNote': 'Activity (automatic)',

  // Composer menu (J-15)
  'lounge.composer.startActivity': 'Start an Activity',
} satisfies Record<string, string>;
