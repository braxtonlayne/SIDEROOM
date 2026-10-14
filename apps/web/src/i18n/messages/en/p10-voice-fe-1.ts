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
} satisfies Record<string, string>;
