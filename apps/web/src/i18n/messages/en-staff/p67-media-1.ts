// Wave 67, media (SEC-AUD-199, D-686): the Live TV channel logo row of the channel editor. Admin strings sit in this
// catalog until `scripts/staff-keys.mjs --write` moves what only staff code names. Two keys it stopped using were removed in wave 68 (SEC-AUD-217).
export default {
  'admin.channelEditor.logo.title': 'Logo',
  'admin.channelEditor.logo.fallback': 'No logo. The guide shows the call sign as text instead.',
  'admin.channelEditor.logo.afterSave': 'Save the channel first, then upload its logo here.',
  'admin.channelEditor.logo.upload': 'Upload logo',
  'admin.channelEditor.logo.remove': 'Remove logo',
  'admin.channelEditor.logo.hint':
    'PNG, JPEG or WebP, up to 2 MiB. It is shown at no more than 512 by 512 pixels and transparency is kept. SVG is not accepted.',
  'admin.channelEditor.logo.tooLarge': 'That file is {size} MiB. A logo can be at most 2 MiB.',
  'admin.channelEditor.logo.wrongType': 'A logo must be a PNG, JPEG or WebP image.',
  'admin.channelEditor.logo.uploaded': 'Logo uploaded.',
  'admin.channelEditor.logo.removed': 'Logo removed.',
} satisfies Record<string, string>;
