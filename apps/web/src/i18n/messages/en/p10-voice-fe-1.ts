export default {
  'voice.controller.micDisconnected':
    'Your microphone was disconnected. Plug it back in and rejoin.',
  'voice.controller.micRecovered':
    'Your microphone was disconnected, so SIDE ROOM switched to your default input.',
  'lounge.pingGraph.collecting': 'Measuring connection',
  'lounge.pingGraph.label': 'Ping over the last readings: average {avg} ms, peak {peak} ms',
  'lounge.pingGraph.stats': 'Avg {avg} ms, peak {peak} ms',
  'voice.capture.cameraBlocked':
    'Camera access is blocked. Allow it in your browser (the lock icon in the address bar), then try again.',
  'voice.capture.noCamera': 'No camera was found. Plug one in or pick another camera.',
  'voice.capture.cameraBusy': 'The camera is busy. Close other apps that use it and try again.',
  'voice.capture.cameraFailed': 'The camera could not be started.',
  'voice.capture.cameraLost': 'Your camera was disconnected, so it was turned off.',
  'voice.capture.screenFailed': 'Screen sharing could not be started.',
  'voice.capture.screenUnsupported': 'Your browser does not support screen sharing.',
  'voice.refusal.videoLimit':
    'This channel has reached its video limit ({limit}). Support this server to raise it, or become a Patron to stream anytime.',
  'voice.refusal.videoLimitNoCount':
    'This channel has reached its video limit. Support this server to raise it, or become a Patron to stream anytime.',
  'voice.refusal.viewerCap':
    'This stream has reached its viewer limit ({limit}). Support this server to raise it.',
  'voice.refusal.viewerCapNoCount':
    'This stream has reached its viewer limit. Support this server to raise it.',
  'lounge.voiceNotices.reduced': 'Reduced quality: the room is busy.',
  'lounge.voiceNotices.support': 'Support this server',
  'lounge.voiceNotices.patron': 'Become a Patron',
} satisfies Record<string, string>;
