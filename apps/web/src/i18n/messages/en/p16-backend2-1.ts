export default {
  'admin.liveTv.slate.title': 'Off-air slate',
  'admin.liveTv.slate.set': 'Asset {id} · one pass of the loop lasts {seconds} s',
  'admin.liveTv.slate.setNoLength': 'Asset {id}',
  'admin.liveTv.slate.notPlayable':
    'Asset {id} is set but has no renditions to play yet (still transcoding, or it failed). Channels show their own card until it plays.',
  'admin.liveTv.slate.none':
    'None. A channel with nothing scheduled shows a card instead of a picture. Make the loop on a machine with ffmpeg:',
  'admin.liveTv.slate.remove': 'To remove it run',
  'admin.liveTv.slate.orLimits': 'or edit the setting under',
  'admin.liveTv.slate.limitsLink': 'Limits',
} satisfies Record<string, string>;
