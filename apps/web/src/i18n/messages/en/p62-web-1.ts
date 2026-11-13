// SEC-AUD-147 (D-638, wave 62): Block, Remove Friend, End Poll and End Stage ask first.
export default {
  'lounge.confirm.blockTitle': 'Block {name}?',
  'lounge.confirm.blockConfirm': 'Block',
  'lounge.confirm.removeFriendTitle': 'Remove {name} as a friend?',
  'lounge.confirm.removeFriendBody': 'You can send a new friend request later.',
  'lounge.confirm.removeFriendConfirm': 'Remove Friend',
  'lounge.confirm.endPollTitle': 'End this poll now?',
  'lounge.confirm.endPollBody': 'Nobody can vote after this. The results stay.',
  'lounge.confirm.endPollConfirm': 'End Poll',
  'lounge.confirm.endStageTitle': 'End the stage for everyone?',
  'lounge.confirm.endStageBody':
    'The stage ends. Everyone in it goes back to the audience and stays in the channel.',
  'lounge.confirm.endStageConfirm': 'End Stage',
} satisfies Record<string, string>;
