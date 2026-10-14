export default {
  'lounge.reactions.personCount': '{n, plural, one {{n} person} other {{n} people}}',
  'lounge.reactions.andOthers':
    '{names} and {rest, plural, one {{rest} other} other {{rest} others}}',
  'lounge.reactions.andLast': '{names} and {last}',
  'lounge.reactions.reactedWith': '{who} reacted with {emoji}',
  'lounge.reactions.pillLabel': '{emoji} {count} {count, plural, one {reaction} other {reactions}}',
  'lounge.reactions.pillLabelMe':
    '{emoji} {count} {count, plural, one {reaction} other {reactions}}, including you',
  'lounge.reactions.addReaction': 'Add reaction',
  'lounge.reactions.addReactionToMessage': 'Add reaction to this message',
  'lounge.reactors.title': 'Reactions',
  'lounge.reactors.peopleReactedWith': ' people reacted with {emoji}',
  'lounge.reactors.panelLabel': 'People who reacted with {emoji}',
  'lounge.reactors.loadError': 'Could not load reactions.',
  'lounge.reactors.loading': 'Loading…',
  'lounge.reactors.removeReaction': "Remove {name}'s reaction",
  'lounge.reactors.remove': 'Remove',
  'lounge.sticker.label': 'Sticker: {name}',
} satisfies Record<string, string>;
