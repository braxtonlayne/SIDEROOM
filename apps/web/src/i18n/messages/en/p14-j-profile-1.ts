// J-06 to J-09: server traits, role colour gradients, View Server As Role, the voice status tab, the soundboard trimmer.
export default {
  // Server traits (J-07, LNG-SSET-007)
  'settings.traits.title': 'Traits',
  'settings.traits.hint':
    'Up to {max} short labels, each with an emoji, shown on your server in the directory and on its profile.',
  'settings.traits.listLabel': 'Server traits',
  'settings.traits.emojiLabel': 'Emoji of trait {n}',
  'settings.traits.labelLabel': 'Label of trait {n}',
  'settings.traits.errorChars': 'A label cannot contain mentions or markdown.',
  'settings.traits.errorEmpty': 'Give the trait a label.',
  'settings.traits.remove': 'Remove trait {n}',
  'settings.traits.removeShort': 'Remove',
  'settings.traits.add': 'Add a trait',
  'settings.traits.previewLabel': 'How the traits look',
  'settings.traits.invalid': 'Every trait needs a label without mentions or markdown.',
  'lounge.directory.traits': 'Traits of {name}',
  'lounge.guildProfile.traits': 'Traits of {name}',

  // Role colour gradients (J-08, LNG-SSET-032)
  'settings.roles.styleLabel': 'Color style',
  'settings.roles.styleSolid': 'Solid',
  'settings.roles.styleGradient': 'Gradient',
  'settings.roles.styleHolographic': 'Holographic',
  'settings.roles.secondColorLabel': 'Second role color',
  'settings.roles.preview': 'Preview:',

  // View Server As Role (J-09, LNG-SSET-036)
  'settings.roles.viewAs': 'View Server As Role',
  'settings.viewAs.title': 'View Server As Role',
  'settings.viewAs.description':
    'See which channels a member who holds only the roles you pick would see. Nothing is saved.',
  'settings.viewAs.close': 'Close',
  'settings.viewAs.rolesLegend': 'Roles',
  'settings.viewAs.noRoles': 'This server has no roles besides @everyone.',
  'settings.viewAs.engineFailed': 'The permission engine could not be loaded.',
  'settings.viewAs.administrator':
    'One of these roles has Administrator, which sees every channel.',
  'settings.viewAs.resultTitle': 'Channels they would see',
  'settings.viewAs.loading': 'Working it out',
  'settings.viewAs.none': 'They would see no channels.',
  'settings.viewAs.resultLabel': 'Visible channels',
  'settings.viewAs.count': '{shown} of {total} channels visible.',

  // Voice status in the channel editor (J-09, LNG-CHN-010)
  'settings.channels.tabVoice': 'Voice',
  'settings.channelTabs.voiceStatusLabel': 'Channel status',
  'settings.channelTabs.voiceStatusHint':
    'A line shown under the channel name while people are in it. It ends when the last person leaves.',
  'settings.channelTabs.voiceStatusSave': 'Set status',
  'settings.channelTabs.voiceStatusClear': 'Clear status',
  'settings.channelTabs.voiceStatusSaved': 'Status set.',
  'settings.channelTabs.voiceStatusCleared': 'Status cleared.',

  // Soundboard trimmer (J-06, D-201)
  'settings.soundboard.trimTitle': 'Trim the sound',
  'settings.soundboard.trimIntro':
    'This sound is {seconds} seconds long and the limit is {max}. Pick the part to keep.',
  'settings.soundboard.trimStart': 'Start of the part to keep',
  'settings.soundboard.trimRange': '{from} s to {to} s',
  'settings.soundboard.trimPlay': 'Play this part',
  'settings.soundboard.trimStop': 'Stop',
  'settings.soundboard.trimUse': 'Use this part',
  'settings.soundboard.trimCancel': 'Cancel trimming',
  'settings.soundboard.trimTooBig':
    'This part would be {kb} KiB as a WAV file; the limit is {max} KiB. Pick another file.',
  'settings.soundboard.ready': 'Ready to upload: {name}',
} satisfies Record<string, string>;
